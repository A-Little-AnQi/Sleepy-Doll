# BetterGI source observations

Static inspection only. Source snapshot: BetterGI `0.64.2-alpha.2`, commit
`63432547cfa97085cca0544eaa93b204b42a19b6`. The project was not built or run.

These notes constrain a future BetterGI Plugin; they are not Agent Kernel behavior.

## Configuration

- `ConfigService` loads `User/config.json` once into static `AllConfig`, attaches
  `PropertyChanged` handlers and rewrites the complete object on changes.
- External file replacement does not update the live object. A later UI property
  change can overwrite an external edit with cached state.
- Global configuration uses System.Text.Json with camel-case names, comments and
  trailing commas accepted, named floating-point values, and OpenCV converters.
- `User/OneDragon/*.json` is loaded and written separately with Newtonsoft.Json.
  Writes are direct `File.WriteAllText`, and several UI change handlers save eagerly.
- Scheduler groups live in `User/ScriptGroup/*.json`. `ScriptGroup.WriteToFileAtomically`
  writes UTF-8 without BOM to a same-directory temporary file, then moves it over the
  destination.

## JavaScript projects and settings

- Projects live under `User/JsScript/<folder>`, with author-owned `manifest.json`, main
  JavaScript and optional `settings_ui` description.
- The author `settings.json` is UI metadata (`SettingItem`); user values are embedded
  as `JsScriptSettingsObject` inside each scheduler-group project.
- Before execution BetterGI removes invalid multi-checkbox values. A script can mutate
  its settings object, and `ScriptService` writes the complete group again in `finally`.
  A Plugin must therefore detect running-group writeback races.
- JavaScript runs in ClearScript V8. Injected hosts include `genshin`, `dispatcher`,
  pathing, key/mouse, limited files, HTTP, notifications, recognition types and direct
  input functions. The future HTTP catalog should expose reviewed typed adapters, not
  mirror every host object blindly.

## Execution and result boundaries

- `TaskControl.TaskSemaphore` is the process-wide exclusive task lock.
- `CancellationContext` owns a mutable global CTS. Some entry paths reset it before
  acquiring the semaphore, so an external execution service must use a single owner
  protocol rather than stack another independent lock around existing runners.
- `TaskRunner` initializes capture/UI state, clears triggers, activates the game,
  releases all keys during cleanup and restores triggers.
- `ScriptService.RunMulti` catches individual script exceptions, logs them and may
  continue. `StartGroups` also catches its outer exception. A group-end message is not
  authoritative proof that every project succeeded.
- Pathing has explicit `SuccessEnd`; JavaScript, key/mouse and shell projects are often
  recorded successful when their call returns, so stronger business verification must
  come from typed HTTP methods and state observations.
- Resin, food, crafting and reward actions exist behind multiple task families. The
  future capability catalog must declare resource costs and verification per method.

## Logs and scheduling

- Serilog writes shared rolling files under `log/`, includes a stable BGI instance
  identity, retains up to 31 files and 21 days, and writes exceptions on following
  lines.
- Execution records are separate daily JSON files under `log/ExecutionRecords`; task
  progress is stored under `log/task_progress` and old unfinished records are deleted
  after three days.
- `ScriptGroupProject.Schedule` and custom-Cron display text exist, but static search
  found no execution-time schedule predicate using that field. External scheduling
  must not assume BetterGI currently enforces it.

## Plugin implications

The future BetterGI Adapter should own all paths, schemas, presentation metadata,
compatibility rules, log parsing and HTTP capability semantics. The generic Core only
receives opaque snapshots, MutationPlans, verification results and diagnostics.
