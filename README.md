# Torch

One torch, carried through five stages.

Torch is a desktop app that turns a single prompt into a multi-stage AI
pipeline: each stage is a separate headless
[Claude Code](https://claude.com/claude-code) invocation running a model and
effort level matched to that stage's cognitive demands. The plan gets
adversarially reviewed by fresh-session critics before a line of code is
written, and an execution-verified refinement loop actually runs your build
and tests before claiming anything works.

## The pipeline

| # | Stage | Default model · effort | Session |
|---|-------|------------------------|---------|
| 1 | **Intake** — asks you 3–5 sharp clarifying questions, writes the brief and the verify commands | sonnet · low | new (becomes the main session) |
| 2 | **Planner** — full spec: stack, modules, contracts, milestones; spawns research subagents when the goal touches fast-moving dependencies | fable · max | resumes main |
| 3 | **Critics** — adversarial review in brand-new sessions (opus + fable in parallel on the 20x tier; single critic otherwise), then a merge pass | opus/fable · high | fresh ×2, merge resumes main |
| 4 | **Implementer** — writes real files into your working directory; Heavy Mode swaps in opus | sonnet · medium | fresh, in the workdir |
| 5 | **Verify ⇄ Refine** — the orchestrator itself runs your build/tests (zero tokens), feeds structured failures back into the implementer's resumed session; the same failure surviving consecutive iterations escalates the refiner to fable | sonnet → fable | resumes implementer |

No stage ever sees only the previous stage's output: every prompt carries the
original goal verbatim plus the accumulated artifact. If the loop can't go
green, Torch says so — it never claims success it didn't verify.

Presets: the standard 5-stage loop, **Classic Linear** (the original 6-stage
telephone pipeline, kept so you can A/B it against the loop on the same
goal), and **Fast** (Plan → Implement → Verify⇄Refine).

## Bring your own Claude subscription

Torch does not use the Anthropic API and never asks for an API key. It drives
your locally installed `claude` CLI, which authenticates through your existing
Claude subscription login. All stages share your subscription rate limits —
the usage footer keeps that visible.

## Run it

Prerequisites: a logged-in `claude` CLI, Rust stable, Node 20+.

```bash
# desktop app (dev)
npm install --prefix ui
cargo install tauri-cli --version "^2"
cargo tauri dev          # from crates/torch-app

# headless engine, no GUI (Fast preset)
cargo build --release -p torch-cli
./target/release/torch --dir /path/to/workdir \
  --verify "cargo test" "Build me a …"
```

Opening `ui` in a plain browser (`npm run dev --prefix ui`) runs **demo
mode** — a scripted fake run for exploring the interface without spending
tokens.

Each run writes `torch/run-<timestamp>/artifact.md` (brief, plan, critiques,
final spec) and per-iteration verify logs into your working directory, so
output survives independent of the app. Run history lives in SQLite.

## Architecture

```
crates/torch-core   engine: claude process supervision, stream-json parsing,
                    session threading, orchestrator (presets, intake Q&A,
                    parallel critics, checkpoints), deterministic verifier
crates/torch-app    Tauri 2 shell: commands, event bridge, SQLite
crates/torch-cli    thin headless front-end over the engine
ui/                 React + TypeScript Control Room (hand-rolled CSS on a
                    design-token layer; Pitch / Iron / Ember themes)
```

The engine is UI-agnostic and fully tested against stub `claude` binaries
replaying real CLI output — `cargo test --workspace` needs no login and no
network.

### The torch is the status system

No spinners, no badges: each stage card carries a torch. **Unlit** = queued,
**lit** (animated flame) = running, **spent** (charred, ember pulse) = done,
**guttered** (low red ember) = failed — the only red in the app. The flame
palette is identical across all three themes; amber always means "work is
happening here". Everything respects `prefers-reduced-motion`.

## License

Dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option. See [CONTRIBUTING.md](CONTRIBUTING.md).
