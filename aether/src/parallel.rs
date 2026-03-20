//! Parallel streaming encryption/decryption using a crossbeam pipeline.
//!
//! Architecture: Reader thread → N worker threads → Writer (main thread).
//!
//! Each chunk is independently encrypted/decrypted using a deterministic nonce
//! derived from (base_nonce, chunk_index, is_last). The writer reassembles
//! chunks in order using a BTreeMap reorder buffer.
//!
//! # Safety note (decryption)
//!
//! Because chunks are decrypted and written independently, plaintext is emitted
//! before the final chunk's AEAD tag has been verified. If the stream is
//! truncated or tampered with, an error is returned after partial output has
//! already been written. Callers should write to a temporary location and only
//! commit the output after a successful return.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::sync::Mutex;

use crossbeam::channel::{Receiver, Sender, bounded};

use crate::algorithm::CipherAlgorithm;
use crate::cipher::{AeadInner, read_exact_or_eof};
use crate::error::{AetherError, Result};
use crate::header::{ChunkKind, KEY_SIZE, MAX_NONCE_SIZE, compute_nonce};

// ──────────────────────────────────────────────────────────────────────────────
// Internal types
// ──────────────────────────────────────────────────────────────────────────────

/// Shared parameters for parallel stream operations.
pub(crate) struct ParStreamParams<'a> {
    pub algorithm: CipherAlgorithm,
    pub key: &'a [u8; KEY_SIZE],
    pub nonce_original: &'a [u8; MAX_NONCE_SIZE],
    pub nonce_size: usize,
    pub chunk_kind: ChunkKind,
    pub first_chunk_ad: &'a [u8],
    pub num_workers: usize,
}

struct ChunkJob {
    index: u64,
    data: Vec<u8>,
    is_last: bool,
}

struct ProcessedChunk {
    index: u64,
    data: Result<Vec<u8>>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Public (crate) API
// ──────────────────────────────────────────────────────────────────────────────

fn resolve_workers(num_workers: usize) -> usize {
    if num_workers == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) } else { num_workers }
}

/// Parallel STREAM encryption.
///
/// Reads plaintext chunks from `r`, encrypts them across `num_workers` threads,
/// and writes ciphertext to `w` in order.
pub(crate) fn par_stream_encrypt<R: BufRead + Send, W: Write>(
    r: &mut R,
    w: &mut W,
    p: &ParStreamParams<'_>,
) -> Result<()> {
    let num_workers = resolve_workers(p.num_workers);
    let pt_size = p.chunk_kind.plaintext_size();
    let reader_err: Mutex<Option<AetherError>> = Mutex::new(None);

    let scope_result = crossbeam::scope(|s| -> Result<()> {
        let (job_tx, job_rx) = bounded::<ChunkJob>(num_workers * 2);
        let (result_tx, result_rx) = bounded::<ProcessedChunk>(num_workers * 2);

        // ── Reader thread ────────────────────────────────────────────────
        let reader_err_ref = &reader_err;
        s.spawn(move |_| {
            if let Err(e) = read_chunks_encrypt(r, &job_tx, pt_size) {
                *reader_err_ref.lock().unwrap() = Some(e);
            }
        });

        // ── Worker threads ───────────────────────────────────────────────
        for _ in 0..num_workers {
            let rx = job_rx.clone();
            let tx = result_tx.clone();
            s.spawn(move |_| {
                let aead = AeadInner::new(p.algorithm, p.key);
                for job in rx {
                    let nonce = compute_nonce(p.nonce_original, p.nonce_size, job.index, job.is_last);
                    let ad = if job.index == 0 { p.first_chunk_ad } else { &[] };
                    let mut work = job.data;
                    let data = aead.encrypt_in_place(&nonce, &mut work, ad).map(|()| work);
                    if tx.send(ProcessedChunk { index: job.index, data }).is_err() {
                        break;
                    }
                }
            });
        }
        drop(job_rx);
        drop(result_tx);

        write_ordered(&result_rx, w)
    });

    scope_result.map_err(|_| AetherError::Encryption("parallel encryption panicked".into()))??;
    if let Some(e) = reader_err.into_inner().unwrap() {
        return Err(e);
    }
    Ok(())
}

/// Parallel STREAM decryption.
///
/// Reads ciphertext chunks from `r`, decrypts them across `num_workers` threads,
/// and writes plaintext to `w` in order.
///
/// **Plaintext is written before the final chunk is verified.** If authentication
/// of any chunk fails, the function returns an error, but earlier chunks may
/// have already been written.
pub(crate) fn par_stream_decrypt<R: BufRead + Send, W: Write>(
    r: &mut R,
    w: &mut W,
    p: &ParStreamParams<'_>,
) -> Result<()> {
    let num_workers = resolve_workers(p.num_workers);
    let ct_size = p.chunk_kind.ciphertext_size();
    let reader_err: Mutex<Option<AetherError>> = Mutex::new(None);

    let scope_result = crossbeam::scope(|s| -> Result<()> {
        let (job_tx, job_rx) = bounded::<ChunkJob>(num_workers * 2);
        let (result_tx, result_rx) = bounded::<ProcessedChunk>(num_workers * 2);

        // ── Reader thread ────────────────────────────────────────────────
        let reader_err_ref = &reader_err;
        s.spawn(move |_| {
            if let Err(e) = read_chunks_decrypt(r, &job_tx, ct_size) {
                *reader_err_ref.lock().unwrap() = Some(e);
            }
        });

        // ── Worker threads ───────────────────────────────────────────────
        for _ in 0..num_workers {
            let rx = job_rx.clone();
            let tx = result_tx.clone();
            s.spawn(move |_| {
                let aead = AeadInner::new(p.algorithm, p.key);
                for job in rx {
                    let nonce = compute_nonce(p.nonce_original, p.nonce_size, job.index, job.is_last);
                    let ad = if job.index == 0 { p.first_chunk_ad } else { &[] };
                    let mut work = job.data;
                    let data = aead.decrypt_in_place(&nonce, &mut work, ad).map(|()| work);
                    if tx.send(ProcessedChunk { index: job.index, data }).is_err() {
                        break;
                    }
                }
            });
        }
        drop(job_rx);
        drop(result_tx);

        write_ordered(&result_rx, w)
    });

    scope_result.map_err(|_| AetherError::Decryption("parallel decryption panicked".into()))??;
    if let Some(e) = reader_err.into_inner().unwrap() {
        return Err(e);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Read plaintext chunks and send them to the job channel.
fn read_chunks_encrypt<R: BufRead>(r: &mut R, tx: &Sender<ChunkJob>, pt_size: usize) -> Result<()> {
    let mut index = 0u64;
    let mut read_buf = vec![0u8; pt_size];

    loop {
        let pos = read_exact_or_eof(r, &mut read_buf)?;
        if pos == 0 && index > 0 {
            break;
        }
        let is_last = pos < pt_size || r.fill_buf()?.is_empty();
        let job = ChunkJob { index, data: read_buf[..pos].to_vec(), is_last };
        if tx.send(job).is_err() {
            break; // writer disconnected
        }
        index += 1;
        if is_last {
            break;
        }
    }
    Ok(())
}

/// Read ciphertext chunks and send them to the job channel.
///
/// Uses read-ahead (`fill_buf`) to definitively detect the last chunk,
/// eliminating the need for the double-nonce trial used in serial decryption.
fn read_chunks_decrypt<R: BufRead>(r: &mut R, tx: &Sender<ChunkJob>, ct_size: usize) -> Result<()> {
    let mut index = 0u64;
    let mut read_buf = vec![0u8; ct_size];

    loop {
        let pos = read_exact_or_eof(r, &mut read_buf)?;
        if pos == 0 {
            if index == 0 {
                // Empty stream after header+keyblock — treat as empty plaintext.
                // Send a single empty last chunk so the writer produces empty output.
                let _ = tx.send(ChunkJob { index: 0, data: vec![], is_last: true });
            }
            break;
        }
        let is_last = pos < ct_size || r.fill_buf()?.is_empty();
        let job = ChunkJob { index, data: read_buf[..pos].to_vec(), is_last };
        if tx.send(job).is_err() {
            break;
        }

        if is_last {
            // Verify no trailing data after the last chunk.
            let mut trail = [0u8; 1];
            let n = r.read(&mut trail)?;
            if n > 0 {
                return Err(AetherError::Decryption("data after last chunk".into()));
            }
            break;
        }
        index += 1;
    }
    Ok(())
}

/// Receive processed chunks and write them in order.
///
/// Uses a BTreeMap to reorder out-of-sequence results from the worker pool.
fn write_ordered<W: Write>(rx: &Receiver<ProcessedChunk>, w: &mut W) -> Result<()> {
    let mut next_index: u64 = 0;
    let mut pending: BTreeMap<u64, Vec<u8>> = BTreeMap::new();

    for chunk in rx {
        let data = chunk.data?;
        if chunk.index == next_index {
            w.write_all(&data)?;
            next_index += 1;
            while let Some(buffered) = pending.remove(&next_index) {
                w.write_all(&buffered)?;
                next_index += 1;
            }
        } else {
            pending.insert(chunk.index, data);
        }
    }
    Ok(())
}
