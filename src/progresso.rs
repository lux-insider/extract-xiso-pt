//! Progresso de uma operação longa: barra no terminal ou, com
//! `--progresso-json`, uma linha JSON por evento em stdout — o mesmo
//! protocolo do iso2god-pt (`fase`, `progresso`, `concluido`, `erro`), para
//! outro programa (o xiso-manager) mostrar a barra sem interpretar cores.

use std::cell::Cell;
use std::time::Instant;

use serde::Serialize;

use crate::terminal::{BarraProgresso, Tema};

#[derive(Serialize)]
#[serde(tag = "evento", rename_all = "snake_case")]
enum Evento<'a> {
    Fase { fase: &'a str, mensagem: &'a str },
    Progresso { bytes: u64, total_bytes: u64, velocidade_bps: f64, eta_segundos: Option<f64>, arquivo: &'a str },
    Concluido { pasta: &'a str, duracao_segundos: f64, mensagem: &'a str },
    Erro { mensagem: &'a str },
}

fn emitir(e: &Evento) {
    if let Ok(linha) = serde_json::to_string(e) {
        println!("{linha}");
    }
}

pub struct Progresso {
    json: bool,
    barra: Option<BarraProgresso>,
    total: u64,
    feito: Cell<u64>,
    inicio: Instant,
    ultimo: Cell<Option<Instant>>,
}

impl Progresso {
    pub fn novo(rotulo: &str, emoji: &str, total: u64, json: bool) -> Self {
        let barra = (!json).then(|| BarraProgresso::nova(Tema::detectar(), rotulo, emoji, total));
        Self { json, barra, total, feito: Cell::new(0), inicio: Instant::now(), ultimo: Cell::new(None) }
    }

    pub fn fase(json: bool, fase: &str, mensagem: &str) {
        if json {
            emitir(&Evento::Fase { fase, mensagem });
        }
    }

    fn metricas(&self) -> (f64, Option<f64>) {
        let seg = self.inicio.elapsed().as_secs_f64().max(1e-3);
        let vel = self.feito.get() as f64 / seg;
        let eta = (vel > 0.0).then(|| self.total.saturating_sub(self.feito.get()) as f64 / vel);
        (vel, eta)
    }

    /// Soma `n` bytes feitos; redesenha no máximo 10 vezes por segundo.
    pub fn avancar(&self, n: u64, arquivo: &str) {
        self.feito.set(self.feito.get() + n);
        let agora = Instant::now();
        if let Some(u) = self.ultimo.get()
            && agora.duration_since(u).as_millis() < 100
            && self.feito.get() < self.total
        {
            return;
        }
        self.ultimo.set(Some(agora));
        let (vel, eta) = self.metricas();
        if self.json {
            emitir(&Evento::Progresso { bytes: self.feito.get(), total_bytes: self.total, velocidade_bps: vel, eta_segundos: eta, arquivo });
        } else if let Some(b) = &self.barra {
            b.atualizar(self.feito.get(), vel, eta, arquivo);
        }
    }

    pub fn terminar(&self, sucesso: bool) {
        if let Some(b) = &self.barra {
            b.finalizar(self.feito.get(), sucesso);
        }
    }

    pub fn concluido(&self, pasta: &str, mensagem: &str) {
        if self.json {
            emitir(&Evento::Concluido { pasta, duracao_segundos: self.inicio.elapsed().as_secs_f64(), mensagem });
        }
    }

    pub fn duracao(&self) -> f64 {
        self.inicio.elapsed().as_secs_f64()
    }
}

pub fn erro_final(mensagem: &str, json: bool) {
    if json {
        emitir(&Evento::Erro { mensagem });
    }
}
