"""Gera docs/banner.png, o banner do extract-xiso-pt: o disco virando pastas e arquivos.

Uso (na raiz do repositório):  python3 ferramentas/gerar_banner.py

Precisa do Pillow e das fontes DejaVu (pacote fonts-dejavu-core no Linux).
"""
import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

MONO = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
NEGRITO = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
CLARO = (232, 240, 245)
W, H = 1600, 640


def brilho(camada, raio, forca):
    g = camada.filter(ImageFilter.GaussianBlur(raio))
    g.putalpha(g.getchannel("A").point(lambda v: int(min(255, v * forca))))
    return g


def cola(im, camada, raio=8, forca=0.8, pos=(0, 0)):
    im.alpha_composite(brilho(camada, raio, forca), pos)
    im.alpha_composite(camada, pos)


def fundo(cor, centro_x=0.36):
    """Degradê radial bem escuro puxando para a cor e grade de pontos."""
    im = Image.new("RGB", (W, H))
    px = im.load()
    for y in range(H):
        for x in range(W):
            k = max(0, 1 - math.hypot((x - W * centro_x) / W, (y - H * 0.5) / H) * 1.6)
            px[x, y] = tuple(int(9 + c * 0.11 * k) for c in cor)
    im = im.convert("RGBA")
    d = ImageDraw.Draw(im)
    ponto = tuple(int(c * 0.22) for c in cor) + (255,)
    for y in range(20, H, 32):
        for x in range(20, W, 32):
            d.point((x, y), fill=ponto)
    return im


def disco(tam, cor, semente=7):
    """O disco de frente com trilhas de 0 e 1, desenhado em 2x e reduzido."""
    rnd = random.Random(semente)
    s = tam * 2
    c = s // 2
    r = int(s * 0.48)
    miolo = int(r * 0.36)
    furo = int(r * 0.13)
    camada = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    corpo = Image.new("RGBA", (s, s))
    px = corpo.load()
    base = tuple(int(30 + 0.12 * v) for v in cor)
    for y in range(s):
        for x in range(s):
            b = (0.5 + 0.5 * math.cos(2 * (math.atan2(y - c, x - c) + 0.7))) ** 3
            px[x, y] = tuple(int(bv + (cv * 0.75 - bv) * b) for bv, cv in zip(base, cor)) + (255,)
    mascara = Image.new("L", (s, s), 0)
    dm = ImageDraw.Draw(mascara)
    dm.ellipse((c - r, c - r, c + r, c + r), fill=255)
    dm.ellipse((c - miolo, c - miolo, c + miolo, c + miolo), fill=0)
    camada.paste(corpo, (0, 0), mascara)

    fonte = ImageFont.truetype(MONO, int(s * 0.022))
    lado = int(s * 0.03)
    raio = r * 0.93
    while raio > r * 0.42:
        n = int(2 * math.pi * raio / (s * 0.0165))
        for i in range(n):
            a = 2 * math.pi * i / n
            t = Image.new("RGBA", (lado, lado), (0, 0, 0, 0))
            ImageDraw.Draw(t).text((lado / 2, lado / 2), rnd.choice("01"), font=fonte,
                                   anchor="mm", fill=cor + (int(150 + 100 * rnd.random()),))
            giro = -math.degrees(a) - 90
            if math.sin(a) > 0:
                giro += 180
            t = t.rotate(giro, resample=Image.BICUBIC)
            camada.alpha_composite(t, (int(c + raio * math.cos(a) - lado / 2),
                                       int(c + raio * math.sin(a) - lado / 2)))
        raio -= s * 0.026

    d = ImageDraw.Draw(camada)
    linha = max(2, s // 300)
    clara = tuple(min(255, int(v * 0.6 + 100)) for v in cor) + (255,)
    d.ellipse((c - r, c - r, c + r, c + r), outline=clara, width=linha)
    d.ellipse((c - miolo, c - miolo, c + miolo, c + miolo),
              fill=tuple(int(v * 0.08 + 12) for v in cor) + (255,), outline=cor + (255,), width=linha)
    d.ellipse((c - furo, c - furo, c + furo, c + furo), fill=(3, 5, 8, 255),
              outline=tuple(int(v * 0.6) for v in cor) + (255,), width=linha)
    return camada.resize((tam, tam), Image.LANCZOS)


def titulo(im, x, nome, sub, cor, tam=84):
    camada = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(camada)
    f1 = ImageFont.truetype(NEGRITO, tam)
    d.text((x, H // 2 - 36), nome, font=f1, fill=CLARO + (255,), anchor="lm")
    d.text((x + 4, H // 2 + 42), sub, font=ImageFont.truetype(MONO, 25), fill=cor + (255,), anchor="lm")
    cola(im, camada, 10, 0.7)


def extract():
    cor = (251, 191, 36)  # âmbar
    im = fundo(cor, 0.3)
    t = 440
    dx, dy = 120, (H - t) // 2
    cola(im, disco(t, cor, semente=11), 26, 0.6, (dx, dy))
    cx, cy = dx + t // 2, dy + t // 2

    # árvore de pastas e arquivos saindo do disco
    arv = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    da = ImageDraw.Draw(arv)
    linha = cor + (200,)
    raiz_x, raiz_y = 700, cy - 165
    itens = [(0, "pasta"), (1, "arq"), (1, "pasta"), (2, "arq"), (2, "arq"), (1, "arq"), (0, "pasta"), (1, "arq")]
    # ligação do disco até a raiz
    da.line((dx + t - 6, cy, raiz_x - 40, cy), fill=linha, width=3)
    da.line((raiz_x - 40, raiz_y, raiz_x - 40, raiz_y + 7 * 48), fill=linha, width=3)
    for i, (nivel, tipo) in enumerate(itens):
        y = raiz_y + i * 48
        x = raiz_x + nivel * 46
        da.line((raiz_x - 40 if nivel == 0 else x - 28, y, x - 6, y), fill=linha, width=3)
        if nivel > 0:
            da.line((x - 28, y - 30, x - 28, y), fill=linha, width=3)
        if tipo == "pasta":
            da.rounded_rectangle((x, y - 15, x + 46, y + 15), radius=4, fill=cor + (230,))
            da.rounded_rectangle((x, y - 21, x + 20, y - 11), radius=3, fill=cor + (230,))
        else:
            da.rounded_rectangle((x, y - 17, x + 30, y + 17), radius=4,
                                 fill=(52, 44, 26, 220), outline=CLARO + (255,), width=2)
            da.polygon([(x + 20, y - 17), (x + 30, y - 7), (x + 20, y - 7)], fill=CLARO + (255,))
        largura = [150, 90, 120, 70, 100][i % 5]
        da.rounded_rectangle((x + 58, y - 4, x + 58 + largura, y + 4), radius=4, fill=(120, 110, 90, 150))
    cola(im, arv, 8, 0.6)

    # selo de conferência
    selo = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ds = ImageDraw.Draw(selo)
    sx, sy, r = 1015, cy + 150, 34
    ds.ellipse((sx - r, sy - r, sx + r, sy + r), fill=(74, 222, 128, 255))
    ds.line((sx - 15, sy + 1, sx - 4, sy + 13, sx + 17, sy - 12), fill=(10, 30, 15, 255), width=8, joint="curve")
    cola(im, selo, 10, 0.8)
    titulo(im, 1085, "extract-xiso", "listar · extrair · verificar", cor, 66)
    return im.convert("RGB")


if __name__ == "__main__":
    destino = Path(__file__).resolve().parent.parent / "docs" / "banner.png"
    extract().save(destino, optimize=True)
    print(destino)
