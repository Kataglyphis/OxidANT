# Language models

`crates/text` (`kataglyphis_text`) generates text through two backends that follow the family's
text contract,
[ANTfrastructure's `docs/language-ai.md` § The text contract](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/language-ai.md#the-text-contract):
messages in, decoded text out as it is generated, a cancel from any thread, and speed figures.
The CLI and the flutter_rust_bridge surface use it; OmniAccelerANT's AI Playground calls the latter, and
[ObservANT's `/ask`](observant.md#questions-about-the-visits) calls it directly.

## The two backends

- **`genai::GenaiGenerator`** runs a GenAI model directory in process on the chain's
  ONNX Runtime GenAI, on Linux and Windows. It loads `libonnxruntime-genai.so` or
  `onnxruntime-genai.dll` at run time, so nothing links it. The library is looked for beside the
  executable, then under `ONNX_GENAI_ROOT` (`bin`, then `lib`), then at the Linux image's
  `/usr/local/lib/onnxruntime-genai/lib`. There is no bare-name fallback, which would take
  whatever copy the loader finds first.
- **`endpoint::Endpoint`** talks to an OpenAI-compatible server: `/v1/chat/completions` with a
  server-sent stream, and `/v1/embeddings`. `EndpointChoice` resolves it in the hub tools' order:
  a URL, `LLM_BASE_URL` or `OLLAMA_BASE_URL`, a backend named in
  [`backends.json`](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/linux/llm-stack/README.md#named-backends),
  then its `default` entry. The registry is the given path or `LLM_BACKENDS`. A backend's
  `api_key_env` names the variable that holds its key; its `request_extra` keys are sent first, so
  the caller's own win.

**The key never reaches a log, and a server cannot grow memory without bound.** `Endpoint`'s
`Debug` prints `api_key` as `Some("<redacted>")`; its implementation destructures the struct, so a
new field fails to compile until it is printed or redacted on purpose. The streamed reader
(`endpoint::read_stream_line`) reads at most `STREAM_LINE_LIMIT` (1 MiB) per line, its `\n` or
`\r\n` aside, and refuses a longer one with `TextError::StreamLineTooLong(limit)` instead of
buffering it. A chunk's JSON sits on one `data:` line, so the bound limits a single piece of text,
not the reply.

**Only the chain's GenAI loads.** As for ONNX Runtime, the loader reads the library and refuses
it unless it embeds the chain's build tree (`/opt/onnxruntime-genai/build/` on Linux,
`C:\temp\onnx-genai-src\build\` on Windows), which its bundled dependencies carry through
`__FILE__`. A pip, NuGet or downloaded copy fails with `LibraryUnavailable`.

## Generating

`TextBackend::generate(messages, options, cancel, on_text)` applies the model's own chat template,
then decodes at most `max_new_tokens` tokens. Temperature 0, the default, decodes greedily; the
endpoint then sends no `top_p` or `top_k`. `on_text` receives each piece on the generating thread.
In GenAI a token that ends inside a UTF-8 character yields no piece until the character completes.

The result holds the text, the token ids (GenAI only), the prompt and completion token counts,
the time to the first token and the decode rate after it. `finish_reason` is `Stop` at an end
token or the server's `stop`, `Length` when the budget ran out, and `Cancelled` after
`CancelToken::cancel`. A cancel ends the run after its current token. An endpoint's run ends within
20 ms even while the server is silent; its connection closes at the server's next line. Each run
clears the token as it starts. A stream that ends without a finish reason is an error, not an
answer. A server without usage reports leaves the completion count at the number of pieces.

**A thinking model's reasoning is not the answer.** It arrives in one of three ways, and
`answer::Answer` handles all of them for both backends:

| Where the thinking is | Who sends it so | What happens |
| --- | --- | --- |
| `delta.reasoning_content` | llama-server, vLLM | `parse_stream_line` yields `StreamEvent::Thinking`, never `Text` |
| `delta.reasoning` | Ollama | the same |
| `<think>...</think>` inside the text | a model in process, a server that does not split it | the block and its tags are cut out of the text |

`on_text` and `result.text` hold the answer only, and the white space a model writes directly
after `</think>` is dropped, so the answer starts with its first word. `result.thinking_chars`
counts the characters that were thinking, the two tags included; 0 means the reply had none. A
block that never closes
is thinking to its end, so a reply cut inside it has an empty `text` and a count above 0: the
caller can say that the budget went into thinking instead of showing nothing. Text that may begin
a tag (`<thi`) is held back from `on_text` until the next piece decides it, as for a stop string,
and is shown at the end of the reply when it was no tag. A thinking delta counts as a piece: the
first one ends the time to the first token, and without a usage report it is in the completion
count. Thinking still costs tokens and time; nothing here switches it off. An endpoint does that
through its backend's `request_extra`, for example `{"chat_template_kwargs": {"enable_thinking":
false}}` on a Qwen server; GenAI applies the chat template without arguments, so a model in
process cannot.

**A `</think>` without its `<think>`** (owner, 2026-11-12): some chat templates, Qwen3's on some
servers, open the block in the prompt, so the reply holds only its end. Everything before that tag
is thinking, as OrchestrANT's `answers.py` reads it: `result.text` is the text after it. `on_text`
has shown the earlier text by then, since nothing could tell it was thinking, and a stop string in
it ends the reply; a caller that must not show thinking reads `result.text`.

Not handled: tags other than `<think>` pass through as text, and an answer that itself writes
`<think>` or `</think>` loses what follows or what came before.

`GenerationOptions` also carries three optional controls, on both backends:

| Option | GenAI | Endpoint |
| --- | --- | --- |
| `seed` | the `random_seed` search option; one seed, model and provider repeat their tokens | `seed` |
| `json_schema` | `OgaGeneratorParamsSetGuidance("json_schema", ...)`, which needs a GenAI built with llguidance | `response_format` with `type: json_schema` and `strict: true` |
| `stop` | the decode loop ends before the first stop string, which the text leaves out; `finish_reason` is `Stop` | `stop` |

`genai::configure` decides every search option and the guidance through the `SearchParams` trait,
so the unit tests record what GenAI would receive without loading it. Text that may begin a stop
string is held back from `on_text` until it is decided; a stop string is looked for in the answer,
not in the thinking. The Linux and the Windows chain both
build GenAI with llguidance; a GenAI built without it refuses a schema with
`TextError::JsonSchemaUnsupported` rather than answering unconstrained.

Each failure has its own `TextError`: the library, the model directory, the provider, the
tokenizer, the chat template, the encoding, invalid options (no messages, a zero budget, a NUL
byte, a seed above `i32::MAX`, an empty stop string, a `json_schema` that is not a JSON object),
generation, an unreachable endpoint, an endpoint HTTP status with its body, a streamed line over
the 1 MiB bound, and a `json_schema` this lane cannot hold.

## Embedding

`GenaiGenerator::embed(texts)` and `Endpoint::embed(texts)` return one L2-normalised vector per
text, in order. An endpoint answers `/v1/embeddings`. GenAI has no embedding call: the generator
encodes each text without the chat template, appends its tokens, which runs the model over all of
them, reads the `hidden_states` output through `OgaGenerator_GetOutput` and takes its mean over the
tokens (`genai::pool_hidden_states`). No token is decoded, and the same generator still answers
`generate`.

The model must be a decoder exported with `--extra_options include_hidden_states=true`. Without that
output `embed` fails with `TextError::EmbeddingsUnsupported`, naming the option. **No released
model has it yet**, SmolLM2-135M-Instruct included, and GenAI 0.17's model builder does not export
BERT, so the hub's multilingual-e5-small and bge-m3 reach OxidANT through an endpoint only. Other
failures: no texts, a NUL byte and a text that encodes to no tokens are `InvalidOptions`, a text at
the model's `context_length` is `GenerationFailed`, and a `hidden_states` output that is not 32-bit
float (a `fp16` export) is `EmbeddingsUnsupported`.

## The CLI

```bash
kataglyphis_cli chat --model-dir <genai-model-dir> --prompt "What is the capital of France?"
kataglyphis_cli chat --backend ollama --model qwen3:4b --prompt "Hallo" --system "Answer in German."
kataglyphis_cli embed --base-url http://localhost:11434 --model nomic-embed-text "first" "second"
kataglyphis_cli embed --model-dir <genai-model-dir> "first" "second"
```

`chat` streams the answer to stdout, without a thinking model's reasoning, and prints the token
count, the time to the first token, the decode rate and the finish reason to stderr; that line
does not name the thinking count yet. `--seed N`, `--json-schema '<schema JSON>'` and a
repeatable `--stop TEXT` set the three options above. `--provider cuda` or `dml` selects GenAI's execution
provider, on `embed --model-dir` too. `embed` prints one L2-normalised vector per text, one JSON array per line.

## The Flutter bridge

`src/api/text.rs` flattens the same calls for Dart:

| Function | What it does |
| --- | --- |
| `text_generate(source, messages, options, sink)` | Streams `TextEvent`s: `piece` with its text, then one `done` with the finish reason and the figures |
| `text_cancel()` | Cancels the running generation, also one whose local model is still loading |
| `text_embed(source, texts)` | The vectors of the model directory or endpoint the source names |
| `text_local_available()` | Whether this build loads the chain's GenAI |
| `text_default_options()` | The contract's defaults |

`TextOptions` carries `seed`, `json_schema` and `stop` as above. A schema on a lane without
llguidance fails `text_generate` with a message starting `json_schema is unsupported on this lane`.
A `piece` holds answer text only. `done` does not carry the thinking count: the generated Dart
code mirrors `TextEvent`, so a new field needs the bindings regenerated.
`TextSource` names either a `model_dir` with a `provider` (`cpu`, `cuda`, `dml`) or the endpoint
fields, plus an `api_key` for platforms without the key's variable. One generation runs at a time,
and the last local model stays loaded for the next turn. Android and macOS reach endpoints only;
the web build reaches language models from Dart, not through this bridge.

## Tests

Only the contract suite loads a released model. The GenAI suites write
[the hub's tiny GenAI model](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/language-ai.md#the-test-model)
with its generator, `third_party/ANTfrastructure/linux/scripts/lib/tiny-genai-model.py`, and read
its `fixture.json` oracle. Greedy decoding of `abc` yields `defg`, and from the byte `0xBF` the
model runs to its end token. Its `--hidden-states` form has the output `embed` reads and the oracle's
vector. Where no GenAI loads they print `SKIP`; CI sets
`KATAGLYPHIS_REQUIRE_GENAI=1`, which turns that into a failure.

`crates/text/tests/contract.rs` replays ModelANT's `text-generation-v1` contract, compiled in from
`third_party/ModelANT/contracts/text-generation-v1.json` at the recorded gitlink, on
SmolLM2-135M-Instruct: greedy decoding of capital-of-france must yield exactly
`[504, 3575, 282, 4649, 314, 7042, 30, 2]` and contain `Paris`. No lane stages the file: the
test binary carries it. `resources/models/llm.lock` pins the archive with OrchestrANT's row;
`scripts/linux/fetch-models.sh --llm` runs the hub's `fetch-data-lock.py --lock
resources/models/llm.lock --extract` in the Linux test and coverage steps, and with
`KATAGLYPHIS_REQUIRE_MODEL=1` a missing model fails. The riscv64 lane runs this suite on scalar
MLAS, because ONNX Runtime's RVV path crashes on this model
([the riscv64 lane](ci.md#the-riscv64-lane)). Windows CI unpacks it into
`target\llm-models` and names that through `KATAGLYPHIS_LLM_MODELS`, because `Build-Windows.ps1`
bundles `resources\`. No installer carries the model. The endpoint suites run against a
local server that plays back one recorded answer and ends within 30 s on its own.

| Suite | What it covers |
| --- | --- |
| `crates/text/src/*.rs` | Option checks, the request body, stream lines and their 1 MiB bound, thinking deltas under both names, `<think>` blocks (whole, split over pieces, unclosed, beside a stop string), the redacted key, embeddings, endpoint resolution, the loader's search and its chain check |
| `crates/text/tests/genai.rs` | The tiny model: the oracle's tokens and text, the end token, a cancel from another thread, invalid requests, a repeated seed's identical ids, at least two different 24-token answers across seeds 1 to 4 at temperature 1, a schema answer that parses and validates on Linux and `JsonSchemaUnsupported` on Windows, a stop string ending with `Stop`, the oracle's embedding, several texts in order, generation after embedding, a model without the output, texts that cannot be embedded |
| `crates/text/tests/contract.rs` | ModelANT's text-generation-v1 on SmolLM2-135M: exact ids and `must_contain` |
| `crates/text/tests/endpoint.rs` | Streaming, cancel, both finish reasons, the three thinking dialects giving one answer, a reply cut inside its thinking, HTTP errors, an over-long stream line, an unreachable port, embeddings, `seed`, `stop` and `response_format` in the request body |
| `crates/cli/tests/text.rs` | `chat` and `embed` on both backends, a named backend, the missing-endpoint error, the `--seed`, `--json-schema` and `--stop` request body |
| `src/api/text.rs` | The bridge's mapping |

Eight mutation recipes in `scripts/linux/tests/ort-license-mutations.json` run on the hosted
mutation lane without GenAI: dropping the seed or the guidance in `genai::configure`, switching
greedy decoding to sampling, dropping the endpoint's `seed` or `response_format`, and in embedding a
last-row vector, an unchecked hidden-states shape and a lost export hint. They do not
cover the FFI calls that apply `configure`'s choices or read `hidden_states`, which only the GenAI suites reach.
