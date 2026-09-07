# Agent Runtime v2

The desktop controller and offline Mock use the same Rust Supervisor. The old
in-memory Agent and TaskRunner have been replaced; `agent.rs` contains compatibility
exports only. Images and multimodal model input are intentionally out of scope.

## Execution

The Supervisor dispatches up to four conversations, with one decision loop per
conversation. Submission is durable and accepts a client-generated idempotency key.
A second message can supplement an active run or queue a separate run. Cancellation
does not call the model. Changing model configuration waits for the current model
request to leave its read guard; the Supervisor and cancellation registry survive.

`plan.update` validates ordered dependencies, semantic capability bindings and
arguments. It then executes the remaining steps in Rust, including authorization,
Job waiting and postcondition verification. A user supplement yields at the next
step boundary. Previously attempted steps cannot be removed or rewritten.

Small independent read-only tool batches run concurrently, four at a time. Scheduling
comes from the shared tool execution contract rather than a tool-name allowlist. The
contract records effect, concurrency safety, result limit and
deferred exposure. Missing or inconsistent plugin metadata fails closed: unknown
effects are serial, require authorization and take the same persistent game lease as
BGI writes. A completed plugin call without authoritative game verification is not
promoted to game success.

Terminal states distinguish `answered` (text delivered), `succeeded` (game effects
verified), `partial`, `failed`, `cancelled`, and `needsReview`. A provider completion
marker by itself cannot establish game success. Limits default to 32 model decisions,
128 tool calls, two plan revisions after the initial plan, 30 minutes, and 100,000
reported/estimated tokens. A context character budget is also applied. Missing model
usage is marked estimated; it is not represented as zero usage.

## Storage and recovery

SQLite stores runs, revisioned plans, attempts, approvals, artifacts, input messages,
events and game leases alongside the legacy transcript. A pre-migration SQLite
backup is made with `VACUUM INTO`, including WAL content. Legacy active tasks become
interrupted; old success records do not receive invented verification evidence.
Configuration v1 is backed up and migrated to v2 while retaining environment-variable
references. Configuration writes use a synchronized temporary file and rename.

State changes use compare-and-swap revisions and append the corresponding event in
the same transaction. Each BGI attempt is persisted before transmission. Its original
wire request and idempotency key are preserved. A lost acceptance can be retransmitted
only when Bridge advertises idempotency; the original key is reused. Without that
feature the result remains unknown and the lease is retained.

On restart, known Jobs are reconciled before continuing. Missing Job identity or a
changed Bridge instance leaves the run in `needsReview`. An explicit `run.resume`
rechecks evidence without manufacturing another game attempt. Unknown cancellation
does not release the game lease. A completed Job remains under its lease until local
postcondition verification has finished.

## Capability and resource descriptors

`runtime.catalogDirectory` defaults to the configuration directory. Its `capabilities/`
contains JSON descriptors with `id`, `description`, `methodId`, `catalogVersion`,
optional `aliases`, `resourceFields` (JSON pointers), and `postconditions`.

Only registered semantic capabilities can be invoked. Raw Bridge method names from
model output do not bypass this binding. The checked-in `mock.*` descriptors work
only with `mock-v1`; they are not real BetterGI capabilities.

`resources/` descriptors contain `id`, `name`, `kind`, a relative `path`,
`contentHash` (SHA-256), optional aliases, coordinate system and map layer. Every
declared resource argument must resolve to an indexed resource with unchanged bytes.
Resource hashes and capability versions are included in authorization bindings and
rechecked before dispatch. Descriptors never execute arbitrary mapping expressions.
This version passes validated arguments through unchanged; coordinate conversion
and richer business argument mapping require explicit trusted adapters.

The first postcondition predicate is `equals`, with `pointer`, `value`, and
`maxAgeSec`. It evaluates only fresh structured observations of the correct instance.
Missing or stale fields produce `unknown`. No screenshot is sent to a model.

Preauthorization uses exact capability/instance/version/argument grants with
`resourceBindingHash` and expiry. Broader range grants are not inferred. Otherwise,
the user approves the bound action in the current conversation. Unknown results
require observation or explicit follow-up, not an automatic fresh write attempt.

## Models, Skills and plugins

The gateway handles OpenAI Responses, Chat Completions, Anthropic Messages, Gemini,
and Ollama. It decodes public text deltas, complete tool calls, finish reasons and
usage. Truncated streams, refusals, invalid argument JSON and missing completion
markers fail before tool execution. Tool names are mapped to portable provider names.
No fallback model or secondary model is selected.

Large tool outputs become run-scoped artifacts at the per-tool declared limit, with
bounded previews and the original character count. Context
compaction retains complete tool-call/result groups and uses an extractive digest of
older user requests. Original transcripts remain stored. It does not ask another
model to summarize, and the digest cannot override structured execution evidence.

Skills have explicit and Chinese bigram matching, version hashes, and bounded
directory-relative reference reading. Disabled Skills are filtered at discovery and
read entrances. Matching bodies are snapshotted for a run; current disable settings
remain effective.

Plugin installation imports a local directory after metadata and file-tree checks.
Imports remain disabled. Updating requires disabling the plugin first. Removal and
replacement retain old files under the plugin directory's `.retired/`. Registry
construction is transactional in memory: a failed plugin cannot leave partial tools.
Enable/disable refreshes the registry; running calls keep their own references. HTTP
tools declare execution policy in their own manifest entry; MCP policies are keyed by
the server's original tool name. Only explicitly read-only, concurrency-safe tools
skip write authorization and participate in parallel batches. Deferred tools are
found through `tools.search`, including their optional search hints. HTTP tools may
also declare an output schema, which is checked before a result reaches the model.

MCP uses Tokio processes, one stdout dispatcher, correlated replies, bounded request
timeouts, cancellation notifications and paginated discovery. Unsupported
server-initiated sampling/filesystem requests receive a method-not-found response.
When an MCP tool declares `outputSchema`, validation is applied to its
`structuredContent` rather than to the surrounding protocol result.
HTTP plugins retain their bounded synchronous compatibility adapter; cancellation
of a call whose termination cannot be established leaves an unknown outcome.

## Desktop contract

- `run.submit`: prompt, optional conversation ID, required stable client key for
  deduplication, optional duration. `task.submit/get/cancel` remain compatibility names.
- `run.get`, `run.cancel`, `run.resume`: operate on the persisted run ID.
- `run.input`: add text for a decision/step boundary or answer a clarification.
- `approval.respond`: one-time response to an unexpired approval ID.
- `events.read`: conversation ID, `after` sequence and bounded `waitMs`; returns up to
  256 ordered events. Long polling waits for events, replacing 800 ms task polling.
- `plugin.install`, `plugin.remove`, `extensions.reload`: local extension lifecycle.

## Reusable operations

The generic Agent Kernel is documented in [agent-kernel.md](./agent-kernel.md). Domain
Plugins describe opaque resources and produce mutation plans; Core owns artifacts,
authorization, leases, commit, verification, compensation and recovery. Core does
not contain a JSON configuration-group editor or domain file schema.

A fully verified deterministic plan can be extracted into the run library. Generic
Workflows bind tool contracts and provider/resource versions. Compatibility BGI
strategies remain readable during migration. Re-running either form does not call a
model or consume model tokens.

The frontend replays events, deduplicates by sequence and discards responses belonging
to a previously selected conversation. Plans, clarification, authorization, queued
messages and recovery remain inside the conversation. Windows close hides to tray;
the tray's explicit exit requests cancellation and allows bounded shutdown time.

## Verification boundaries

Unit and integration tests cover idempotency, stale revisions, persisted leases,
approval expiry, queues, cancellation, lost acceptance, truncated streams, context
limits, resource changes, observation freshness, restart reconciliation, deterministic
multi-step execution, and out-of-order/paginated MCP replies. Mock faults are configured
in code through `MockFaults`, not exposed as production controls.

Real-model API validation and real BetterGI contracts still require configured services.
Bridge SSE is a negotiated wake-up/reconnect channel; authoritative Job reads remain
the reconciliation source. Unsupported features fall back to bounded polling.
Windows cross-compilation validates the tray code, but does not substitute for running
the native window and tray on Windows. No distribution or automatic update workflow
is included in this runtime change.
