# PERSONAL LLM SANCTUM

Local-first adaptive multi-LLM orchestration runtime designed for a Galaxy Tab S9+ / Termux control node with optional remote compute.

SANCTUM is not a claim of autonomous background intelligence. It is an observable execution system: every run has an ID, selected path, candidate outputs, verification state, memory retrieval, timing, and a SQLite journal record.

## Core execution contract

```text
User
  ↓
Intent Parser
  ↓
Adaptive Router
  ├── FAST: one selected model
  └── DELIBERATE: parallel candidate models
                       ↓
                 Cross-model Critic
                       ↓
                    Verifier
                       ↓
                   Synthesizer
                       ↓
             Evidence + Memory + Answer
                       ↓
                      User
```

## Current v0.1 capabilities

- Rust/Axum local runtime bound to `127.0.0.1:8787`
- intent detection for general, analysis, coding, research, creative, and critic work
- adaptive fast vs deliberate routing
- parallel candidate execution in deliberate mode
- separate critic, verifier, and synthesizer stages
- explicit SQLite/WAL memory and execution journal
- run replay endpoint
- generic OpenAI-compatible provider adapter
- deterministic mock providers for end-to-end tests without pretending they are real model validation
- local tablet web UI / PWA shell
- GitHub Actions: format, check, test, clippy

## Termux quick start

```bash
cd ~/PERSONAL-LLM-SANCTUM
cargo test
./scripts/sanctum build
./scripts/sanctum start
```

Then open in the tablet browser:

```text
http://127.0.0.1:8787
```

Status and logs:

```bash
./scripts/sanctum status
./scripts/sanctum logs
```

Stop:

```bash
./scripts/sanctum stop
```

For development:

```bash
./scripts/sanctum dev
```

## API

### Health

```bash
curl http://127.0.0.1:8787/health
```

### Fast path

```bash
curl -s -X POST http://127.0.0.1:8787/v1/chat \
  -H 'content-type: application/json' \
  -d '{"input":"Explain this architecture","mode":"fast"}'
```

### Deliberate path

```bash
curl -s -X POST http://127.0.0.1:8787/v1/chat \
  -H 'content-type: application/json' \
  -d '{"input":"Analyze and compare a production distributed AI architecture","mode":"deliberate"}'
```

### Save explicit memory

```bash
curl -s -X POST http://127.0.0.1:8787/v1/memory \
  -H 'content-type: application/json' \
  -d '{"content":"Important durable context","tags":"project architecture"}'
```

### Search memory

```bash
curl -s 'http://127.0.0.1:8787/v1/memory?q=architecture&limit=5'
```

### Replay one run

```bash
curl -s http://127.0.0.1:8787/v1/runs/<RUN_ID>
```

## Real model configuration

Secrets are never committed. Copy the environment template only as a reference and export keys in your shell or a secure secret manager.

```bash
export OPENAI_API_KEY='...'
```

Then edit `config/sanctum.toml`: enable the provider, set a valid model name, and assign its SANCTUM model alias to one or more router roles.

The `openai_compatible` adapter expects a provider exposing a Chat Completions-compatible endpoint under:

```text
<base_url>/chat/completions
```

This allows SANCTUM to connect to compatible cloud gateways or a compatible local server without coupling the orchestration kernel to one vendor.

## Storage

Runtime state is intentionally excluded from Git:

```text
data/sanctum.db
```

SQLite uses WAL mode. The `runs` table records execution provenance; the `memories` table stores only explicit memory submitted through the API/UI.

## Security baseline

- server binds to loopback by default
- no API keys in source or config committed to Git
- no permissive cross-origin access
- mock verification is explicitly labeled as mock
- external factual verification is not claimed unless an actual evidence source is connected

## Roadmap

1. Direct provider adapters and capability registry
2. cost/latency/quality-aware model scoring
3. uncertainty/disagreement-driven adaptive compute allocation
4. structured verifier verdicts and evidence objects
5. semantic memory retrieval with locally controlled embeddings
6. remote worker protocol for heavy inference
7. health/retry/failover policies
8. CHIMERA-OMEGA evidence/verdict integration
9. encrypted secret handling and scoped provider permissions
10. reproducible evaluation harness for router decisions

## Design rule

SANCTUM should allocate more computation only when the task, uncertainty, disagreement, or requested assurance justifies it. The goal is reliable cognition orchestration, not maximum model calls.
