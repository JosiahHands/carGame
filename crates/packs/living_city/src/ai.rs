//! Optional GGUF inference on a worker, off the game and render threads.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
#[cfg(feature = "local-ai")]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use bevy::prelude::Resource;
use serde_json::Value;

use crate::protocol::{ModelChoice, ThinkKind};

const REQUEST_LIFETIME: Duration = Duration::from_secs(45);

pub struct Request {
    pub kind: ThinkKind,
    pub ids: Vec<String>,
    pub prompt: String,
}

type Answer = Result<Value, String>;

#[derive(Default, Resource)]
pub struct LocalAi {
    path: Option<PathBuf>,
    #[cfg(feature = "local-ai")]
    gpu: bool,
    answer: Option<Mutex<mpsc::Receiver<Answer>>>,
    started: Option<Instant>,
    #[cfg(feature = "local-ai")]
    worker: Option<mpsc::SyncSender<Job>>,
    #[cfg(feature = "local-ai")]
    cancel: Option<Arc<AtomicBool>>,
    download: Option<Download>,
    pub status: String,
}

#[cfg(feature = "local-ai")]
struct Job {
    path: PathBuf,
    gpu: bool,
    request: Request,
    started: Instant,
    answer: mpsc::Sender<Answer>,
    cancel: Arc<AtomicBool>,
}

struct Download {
    child: Child,
    partial: PathBuf,
    path: PathBuf,
}

impl Drop for Download {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.partial);
    }
}

fn model_dir() -> Option<PathBuf> {
    game::storage::data_dir().map(|p| p.join("packs/npc/models"))
}

fn gguf(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0; 4];
    std::fs::File::open(path).and_then(|mut file| file.read_exact(&mut magic)).is_ok() && &magic == b"GGUF"
}

impl LocalAi {
    pub fn model(choice: ModelChoice) -> Option<PathBuf> {
        let file = choice.info().file;
        Self::owned_model(choice).or_else(|| {
            import::old_app_folders()
                .into_iter()
                .map(|p| p.join("packs/npc/models").join(file))
                .find(|p| gguf(p) && p.with_extension("gguf.done").is_file())
        })
    }

    pub fn owned_model(choice: ModelChoice) -> Option<PathBuf> {
        let path = model_dir()?.join(choice.info().file);
        gguf(&path).then_some(path)
    }

    pub fn select(&mut self, choice: ModelChoice, enabled: bool, gpu: bool) {
        self.path = enabled.then(|| Self::model(choice)).flatten();
        #[cfg(feature = "local-ai")]
        {
            self.gpu = gpu;
        }
        #[cfg(not(feature = "local-ai"))]
        let _ = gpu;
    }

    pub fn stop(&mut self) {
        self.path = None;
        self.cancel_request();
        #[cfg(feature = "local-ai")]
        {
            self.worker = None;
        }
    }

    pub fn cancel_request(&mut self) {
        #[cfg(feature = "local-ai")]
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.answer = None;
        self.started = None;
    }

    pub fn busy(&self) -> bool {
        self.answer.is_some()
    }

    pub fn request(&mut self, request: Request) -> bool {
        #[cfg(not(feature = "local-ai"))]
        {
            let Request { kind, ids, prompt } = request;
            drop((kind, ids, prompt));
            false
        }
        #[cfg(feature = "local-ai")]
        {
            let Some(path) = self.path.clone() else { return false };
            if self.busy() {
                return false;
            }
            if self.worker.is_none() {
                let (tx, rx) = mpsc::sync_channel(1);
                if std::thread::Builder::new()
                    .name("living-city-ai".into())
                    .spawn(move || run(rx))
                    .is_err()
                {
                    return false;
                }
                self.worker = Some(tx);
            }
            let (answer, receive) = mpsc::channel();
            let started = Instant::now();
            let cancel = Arc::new(AtomicBool::new(false));
            let job = Job {
                path,
                gpu: self.gpu,
                request,
                started,
                answer,
                cancel: cancel.clone(),
            };
            let Some(worker) = &self.worker else { return false };
            match worker.try_send(job) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(_)) => return false,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.worker = None;
                    return false;
                }
            }
            self.cancel = Some(cancel);
            self.answer = Some(Mutex::new(receive));
            self.started = Some(started);
            self.status = "Thinking locally…".into();
            true
        }
    }

    pub fn poll(&mut self) -> Option<Answer> {
        use mpsc::TryRecvError::{Disconnected, Empty};
        let result = match self.answer.as_mut()?.get_mut().ok()?.try_recv() {
            Ok(result) => result,
            Err(Disconnected) => Err("The AI worker stopped.".into()),
            Err(Empty) if self.started.is_some_and(|t| t.elapsed() >= REQUEST_LIFETIME) => {
                Err("The model took too long. Using the built-in brain.".into())
            }
            Err(Empty) => return None,
        };
        self.cancel_request();
        self.status = match &result {
            Ok(_) => "Local AI ready".into(),
            Err(error) => error.clone(),
        };
        Some(result)
    }

    pub fn start_download(&mut self, choice: ModelChoice) {
        if self.download.is_some() || Self::model(choice).is_some() {
            return;
        }
        let Some(dir) = model_dir() else {
            self.status = "The model folder is unavailable.".into();
            return;
        };
        let info = choice.info();
        let path = dir.join(info.file);
        let partial = path.with_extension("gguf.part");
        let url = format!("https://huggingface.co/{}/resolve/main/{}", info.repo, info.remote_file);
        let result = std::fs::create_dir_all(dir).and_then(|()| {
            Command::new("curl")
                .args([
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--retry",
                    "2",
                    "--proto",
                    "=https",
                    "--proto-redir",
                    "=https",
                    "--output",
                ])
                .arg(&partial)
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        });
        match result {
            Ok(child) => self.download = Some(Download { child, partial, path }),
            Err(error) => self.status = format!("Could not start model download: {error}"),
        }
    }

    pub fn downloading(&self) -> bool {
        self.download.is_some()
    }

    pub fn cancel_download(&mut self) {
        if self.download.take().is_some() {
            self.status = "Download canceled.".into();
        }
    }

    pub fn poll_download(&mut self) {
        let Some(download) = &mut self.download else { return };
        let result = match download.child.try_wait() {
            Ok(None) => {
                let bytes = std::fs::metadata(&download.partial).map_or(0, |m| m.len());
                self.status = format!("Downloading model… {:.0} MB", bytes as f64 / 1_000_000.0);
                return;
            }
            Ok(Some(exit)) if exit.success() && gguf(&download.partial) => {
                std::fs::rename(&download.partial, &download.path).map_err(|e| e.to_string())
            }
            Ok(Some(exit)) => Err(format!("The model download failed ({exit}).")),
            Err(error) => Err(error.to_string()),
        };
        self.download = None;
        self.status = result.err().unwrap_or_else(|| "Model downloaded".into());
    }

    pub fn remove_model(&mut self, choice: ModelChoice) {
        self.stop();
        let result = model_dir().map(|dir| std::fs::remove_file(dir.join(choice.info().file)));
        self.status = match result {
            Some(Ok(())) => "Model removed.".into(),
            Some(Err(error)) => format!("Could not remove model: {error}"),
            None => "The model folder is unavailable.".into(),
        };
    }
}

#[cfg(feature = "local-ai")]
fn run(rx: mpsc::Receiver<Job>) {
    use llama_cpp_2::{
        llama_backend::LlamaBackend,
        model::{LlamaModel, params::LlamaModelParams},
    };
    let mut backend = None;
    let mut model = None;
    while let Ok(job) = rx.recv() {
        if job.cancel.load(Ordering::Relaxed) {
            continue;
        }
        let result = (|| {
            if backend.is_none() {
                backend = Some(LlamaBackend::init().map_err(|e| e.to_string())?);
            }
            let backend = backend.as_ref().expect("initialized above");
            if !model.as_ref().is_some_and(|(path, gpu, _)| path == &job.path && *gpu == job.gpu) {
                model = None;
                let layers = if job.gpu && backend.supports_gpu_offload() { u32::MAX } else { 0 };
                let params = LlamaModelParams::default().with_n_gpu_layers(layers);
                let loaded = LlamaModel::load_from_file(backend, &job.path, &params).map_err(|e| e.to_string())?;
                model = Some((job.path.clone(), job.gpu, loaded));
            }
            generate(backend, &model.as_ref().expect("loaded above").2, &job)
        })();
        let _ = job.answer.send(result);
    }
}

#[cfg(feature = "local-ai")]
fn generate(backend: &llama_cpp_2::llama_backend::LlamaBackend, model: &llama_cpp_2::model::LlamaModel, job: &Job) -> Answer {
    use llama_cpp_2::{context::params::LlamaContextParams, llama_batch::LlamaBatch, sampling::LlamaSampler};
    use std::num::NonZeroU32;
    let request = &job.request;
    let grammar = llama_cpp_2::json_schema_to_grammar(&crate::protocol::schema_for(request.kind, &request.ids).to_string())
        .map_err(|e| e.to_string())?;
    let guarded_prompt = request.prompt.replace("<|", "< |");
    // All three choices are Qwen3.5 models with the same chat markers. An empty
    // thinking block starts the assistant directly in its structured reply.
    let prompt = format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n",
        crate::protocol::SYSTEM_PROMPT,
        guarded_prompt
    );
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
    let max = request.kind.max_tokens();
    let size = (tokens.len() + max + 1).next_multiple_of(256).clamp(1024, 8192);
    if tokens.is_empty() || tokens.len() + max >= size {
        return Err("The resident's prompt was too long.".into());
    }
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(2).clamp(1, 4)) as i32;
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(size as u32))
        .with_n_batch(512)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut context = model.new_context(backend, params).map_err(|e| e.to_string())?;
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::grammar(model, &grammar, "root").map_err(|e| e.to_string())?,
        LlamaSampler::top_p(0.9, 1),
        LlamaSampler::temp(request.kind.temperature()),
        LlamaSampler::dist(u32::MAX),
    ]);
    let mut batch = LlamaBatch::new(512, 1);
    for (part, chunk) in tokens.chunks(512).enumerate() {
        if job.cancel.load(Ordering::Relaxed) {
            return Err("Canceled".into());
        }
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let pos = part * 512 + i;
            batch
                .add(*token, pos as i32, &[0], pos + 1 == tokens.len())
                .map_err(|e| e.to_string())?;
        }
        context.decode(&mut batch).map_err(|e| e.to_string())?;
    }
    let mut out = Vec::new();
    for i in 0..max {
        if job.cancel.load(Ordering::Relaxed) {
            return Err("Canceled".into());
        }
        if job.started.elapsed() >= REQUEST_LIFETIME {
            return Err("The model took too long.".into());
        }
        let token = sampler.sample(&context, -1);
        if vocab.is_eog(token) {
            break;
        }
        vocab.token_to_piece_into(token, &mut out, false, None);
        if out.last() == Some(&b'}')
            && let Ok(data) = serde_json::from_slice(&out)
        {
            return Ok(data);
        }
        batch.clear();
        batch.add(token, (tokens.len() + i) as i32, &[0], true).map_err(|e| e.to_string())?;
        context.decode(&mut batch).map_err(|e| e.to_string())?;
    }
    serde_json::from_slice(&out).map_err(|_| "The model did not finish its reply. Using the built-in brain.".into())
}

#[cfg(all(test, feature = "local-ai"))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a downloaded Qwen3.5 GGUF model"]
    fn local_model_can_generate_a_structured_chat_reply() {
        let path = std::env::var_os("CAR_GAME_TEST_MODEL")
            .map(PathBuf::from)
            .or_else(|| LocalAi::model(ModelChoice::Light))
            .expect("set CAR_GAME_TEST_MODEL to a Qwen3.5 GGUF file");
        let mut ai = LocalAi {
            path: Some(path),
            gpu: true,
            ..Default::default()
        };
        assert!(ai.request(Request {
            kind: ThinkKind::Chat,
            ids: vec!["ava".into()],
            prompt: "You are Ava, an enthusiastic barista. The driver says hello. Say a short greeting.".into(),
        }));
        let reply = loop {
            if let Some(result) = ai.poll() {
                break result.expect("the local model generated JSON");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mind = crate::mind::Mind::new(crate::cast::member("ava").unwrap());
        assert!(crate::brain::read_chat(&reply, &mind).is_some(), "{reply}");
    }
}
