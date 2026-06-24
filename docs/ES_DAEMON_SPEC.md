# ES-Daemon: kernel-level exec block — implementation spec for Claude Code

**Goal:** Close the TOCTOU gap. Today's JS slices (kqueue `chmod 0o000`, chokidar)
run in the same user-space as a potential attacker — malware running as the user
can `chmod +x` + `exec` a dropped file faster than the watcher reacts. The only
real fix is a kernel `AUTH_EXEC` deny held until the scan verdict is known.

This work must be done on the Mac (Swift build, Endpoint Security API, entitlement,
real exec-block testing). The Linux dev sandbox cannot build or run it.

Current state: `es-daemon/Sources/ESWatcher/main.swift` is a ~88-line skeleton that
denies exec by path prefix only. It is NOT wired to the scan verdict, NOT wired into
the main pipeline, and has correctness bugs. This spec lists exactly what to change.

---

## Threat model (be honest about scope)

- **In scope:** a file dropped into the watch dir (or sitting in quarantine) must be
  unable to `exec` until the pipeline marks it `clean`. Covers both "user
  double-clicks too early" and "already-resident malware tries to launch a fresh drop".
- **Out of scope:** malware with root, or that exploits a kernel bug, or that runs
  via an interpreter (`bash evil.sh`, `python evil.py`) where the *interpreter* is the
  exec target, not the script. Document this limit — AUTH_EXEC sees the binary being
  executed, not script arguments. Interpreted payloads need an `AUTH_OPEN` /
  content-scan layer, which the existing quarantine `chmod 0o000` already partly covers.

---

## Tasks, in order

### 1. Fix the deny logic: verdict-driven, not path-prefix-only

The skeleton allows exec outside `watchPath` and denies inside it. That is wrong for
this pipeline because clean files are **restored back into watchPath** (see
`src/watcher.ts` → `fileMover.restoreToWatch`). Required behavior:

- Maintain an in-memory set/map of **allowed paths** (cleared files) keyed by absolute
  path. Default decision for anything under `watchPath` OR `quarantinePath` is **DENY**.
- A path becomes allowed only when the pipeline reports verdict `clean` for it.
- Allowed entry should be consumed/expired (e.g. allow once, or TTL) so a later
  malicious file reusing the same path can't ride a stale allow.
- Anything not under watchPath/quarantinePath → ALLOW (don't become a system-wide
  exec gate; that's a different, much riskier product).

Config: read both `watchPath` and `quarantinePath` (today only watchPath is read).
Pass both as args or env (`WATCH_PATH`, `QUARANTINE_PATH`).

### 2. Wire the verdict channel (the core of the fix)

The Swift daemon must learn "path X is now clean". Pick the lowest-latency option:

- **Preferred:** a local UNIX-domain socket the daemon listens on; the Node pipeline
  pushes `{ "allow": "<absolute-path>" }` when a job reaches verdict `clean`
  (in `src/watcher.ts`, right where it currently calls `fileMover.restoreToWatch`
  for the clean branch — both the cache-hit clean branch ~line 300 and the
  post-VT clean branch ~line 390). Add a small `es-bridge.ts` that opens the socket
  and sends the allow message; call it from those two clean branches.
- Avoid making the daemon poll `/api/jobs` over HTTP inside the AUTH_EXEC callback —
  see task 3 (deadline).

### 3. Respect the AUTH_EXEC deadline — never block in the callback

`AUTH_EXEC` has a hard kernel deadline (~deci-seconds). If `es_respond_auth_result`
is late, the kernel **kills the client** and your protection silently dies.

- The callback must be O(1): consult the in-memory allowed-set and respond
  immediately. No socket I/O, no HTTP, no disk in the callback path.
- The allowed-set is populated asynchronously by the socket listener (task 2) on a
  separate dispatch queue.
- On unknown path under watch/quarantine → respond **DENY** immediately (fail-closed),
  not "wait". The dropped file simply can't run until it's cleared; that's the point.
- Use `es_respond_auth_result(..., cache: false)` so each exec is re-evaluated
  (don't let the kernel cache an ALLOW for a path you may later want to deny).

### 4. Mute your own pipeline

The Node pipeline runs `xattr`, `chmod`, possibly spawns a VT child process. Don't
authorize your own machinery:

- `es_mute_process` (or audit-token mute) the FileSandbox Node process and any child
  it spawns, so their execs/operations don't flow through the auth path. Reduces load
  and removes a self-deadlock risk.

### 5. Correctness fixes in main.swift

- Remove `client!` force-unwrap inside the handler; capture the client safely (the
  handler closure can reference a stored non-optional after successful creation, or
  guard it).
- Handle `es_subscribe` return value (it can fail).
- Add graceful shutdown: `SIGTERM`/`SIGINT` → `es_unsubscribe_all` + `es_delete_client`
  before exit, so a restart doesn't hit `ERR_TOO_MANY_CLIENTS`.
- Log decisions (allow/deny + path) to a file the menu bar / `/api/security-events`
  can surface, consistent with the launch-agent monitor feed already added.

### 6. Provisioning & run model

- The daemon needs `com.apple.developer.endpoint-security.client` entitlement
  (Apple approval) **or** dev mode with SIP disabled. NOTE: this machine currently has
  **SIP enabled** — for local dev you'll either disable SIP in Recovery, or get the
  entitlement. Document whichever path you choose in the README.
- Must run as **root** (`ES_NEW_CLIENT_RESULT_ERR_NOT_PRIVILEGED` otherwise).
- Install as a **LaunchDaemon** (not LaunchAgent — needs root + system context),
  with `KeepAlive` so a crash restarts it. Add the plist under `scripts/`.
- Sign with hardened runtime + the entitlement; `codesign` step in `build.sh`.

### 7. Integration into the main pipeline

- Add config flags to `src/config.ts` (`RawConfig` + `config`): `esEnabled` (bool),
  `esSocketPath` (string, default e.g. `/var/run/filesandbox-es.sock`).
- On startup in `src/index.ts`, if `esEnabled`, verify the daemon is reachable
  (socket exists + a ping), and **fail-closed or warn** per a configurable mode,
  mirroring the existing `pompelmi` probe gate at index.ts:18-32.
- The ES daemon is the *enforcement* layer; the Node pipeline is the *decision* layer.
  Keep that separation clean.

---

## Verification (must do on the Mac)

1. **Negative test:** drop an executable (e.g. `chmod +x` a tiny compiled binary) into
   watchPath, try to run it before scan completes → must be **denied** by kernel
   (`Operation not permitted` / process refused), and `ES: BLOCKED` logged.
2. **Positive test:** let a known-clean file complete the pipeline → after the `allow`
   message, exec must **succeed**.
3. **Deadline test:** induce slowness in the verdict channel; confirm the callback
   still responds within deadline (DENY) and the client is NOT killed
   (no `ES_NEW_CLIENT` re-init in logs).
4. **Self-mute test:** confirm the Node pipeline's own `xattr`/`chmod`/VT-child execs
   are not blocked and don't appear in the auth path.
5. **Restart test:** `launchctl kickstart -k` the daemon; confirm clean
   unsubscribe/restart, no `ERR_TOO_MANY_CLIENTS`.
6. **Interpreter caveat test:** confirm (and document) that `bash watched/evil.sh`
   is NOT blocked by AUTH_EXEC — proves the documented scope limit is real.

---

## Files likely touched

- `es-daemon/Sources/ESWatcher/main.swift` — rewrite deny logic, socket listener,
  mute, deadline-safe handler, shutdown.
- `es-daemon/build.sh` — add codesign + entitlement step.
- `es-daemon/ESWatcher.entitlements` — new file.
- `scripts/<launchdaemon>.plist` — new, root LaunchDaemon.
- `src/es-bridge.ts` — new, Node→daemon allow channel.
- `src/watcher.ts` — call es-bridge in the two `clean` branches.
- `src/config.ts` — `esEnabled`, `esSocketPath`.
- `src/index.ts` — startup probe + wire.
- `README.md` — update ES section from "optional" to real, document scope limits.

## Don'ts (per repo CLAUDE.md)

- No speculative abstractions. Smallest change that closes the gap.
- Don't touch unrelated code. Match existing style.
- Before finishing: typecheck, lint, no stray `console.log` in production paths,
  and run the verification list above on real hardware.
