# Download ownership and handoff

The desktop, browser extension and engine are independent projects. Each owns its
source, dependencies, artifacts and tests. Their parent directory is only a local
workspace. No shared runtime package or cross-repository test harness is required.

## Responsibilities

| Owner             | Responsibility                                                                                                             |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Browser extension | Intercept browser intent, capture observed names and request context, hold the browser download until the desktop answers. |
| Rust desktop      | Apply application preferences, verify handoffs with the engine, hold confirmations, create/control tasks, persist history. |
| Vue desktop       | Edit user intent, present native task state, select content and invoke named commands.                                     |
| Aria2 Next        | Resolve final output names and paths, handle conflicts and recovery, transfer data and publish completed media.            |

`aria2/rpc.rs` owns HTTP JSON-RPC framing, authentication and structured errors.
`services/tasks/` owns task queries, controls and visibility policy.
`services/downloads/` owns ordinary submission, including manual URI/torrent
creation. `services/media/` owns media inspection and selection. Policy does not
belong in the RPC transport. OS deep-link parsing still feeds the desktop dialog;
browser HTTP submissions run in Rust without requiring a mounted WebView.

## Output names

The editable name is one basename: native submission rejects paths, separators,
reserved names and control characters. The save directory remains a separate input.
The dialog displays browser metadata or the URL name without making it an explicit
override. Its lookup button requests HTTP response headers with cancellation and a
10-second timeout, then calls the engine's `aria2.resolveFilename`; it creates no task.

Completed, inactive single files can be renamed from task details. Native exclusive
rename preserves existing destination files. A SQLite intent records filesystem work
before it starts and reconciles an interrupted history update at startup. Active and
seeding tasks retain engine ownership and cannot be renamed through this operation.

For a new ordinary HTTP task, the engine applies this precedence:

1. An explicit user `out` or an already persisted output path.
2. A browser-resolved `filename-hint` with source `browser`.
3. The final payload response's `Content-Disposition` (`filename*` before `filename`).
4. A `suggested` filename hint, then the final URL basename, then the engine default.

Hints are decoded text, not paths. The engine applies its path rules. URL components
are decoded once; literal percent sequences in names, passwords and restored paths
remain literal. The desktop makes no separate HEAD/GET request to guess a name.
Remote torrent metainfo inspection remains a real download, not a naming probe.

For media, `filename-hint-source=title` preserves dots in a page title and appends
the selected container extension. Filename hints and explicit `out` values have
their extension replaced by the selected container. The engine owns publication
and collision handling. Vue never rewrites the native output name on selection.
Directory classification uses the native `services/downloads/category.rs` policy
for browser submissions and frontend previews/submissions. Rules declare a
`directoryMode` of `relative` (under the current default directory) or `absolute`
(a fixed directory). Built-in rules store relative subdirectories. An explicit
per-task directory bypasses classification. File sets must resolve to one directory;
mixed or unclassified content keeps its base directory. Classification uses the
available URL, filename hint or torrent metadata, without a separate naming request.
Completed files are never moved by the WebView, and native output paths remain
authoritative. Rules without the current directory mode must be configured again.

The header rules follow [RFC 6266](https://www.rfc-editor.org/rfc/rfc6266.html).
The existing aria2 header parser and libcurl handle protocol decoding. A bounded
encoded-word adapter is confined to HTTP header values from nonconforming servers;
it is not a general charset detector or a second pass over browser names.

## Ordinary browser API, protocol 3

The desktop owns the contract in `services/downloads/contracts.rs`. The extension
validates the fields it consumes in its own repository. Both endpoints use the
configured Extension API authentication and origin policy; the body limit is 256 KiB.

`GET /downloads/capabilities` answers `{ "product": "rayburst", "protocolVersion": 3 }`
once the engine responds. A client checks it before each handoff; there is no legacy
fallback protocol.

`POST /add` accepts this shape (optional fields may be omitted):

```json
{
  "url": "https://example.com/download",
  "filename": "report%20.pdf",
  "filenameSource": "browser",
  "requestHeaders": [],
  "verify": true,
  "mime": "application/pdf",
  "size": 1048576
}
```

Optional fields are `finalUrl`, `referer`, `cookie`, `userAgent`, `filename`,
`filenameSource` (`browser` or `suggested`, default `suggested`), `requestHeaders`
(`{name,value}` pairs) and, for downloads the browser holds, `verify` with the MIME
type and size the browser received. Supported inputs are HTTP(S), SFTP, magnet, ED2K
and Thunder.

The browser keeps holding its download until the answer, and only the answer
decides who downloads it:

- `{ "action": "added", "gid": … }`: the engine owns the download.
- `{ "action": "cancelled" }`: the user declined it in the add dialog.
- `{ "action": "fallback", "reason": … }`: the browser continues. Reasons are
  `unreachable` (no response), `rejected` (HTTP error), `web-page` (the engine got
  HTML where the browser got a file, typically a missing session), `size-mismatch`
  and `unconfirmed` (the dialog was not answered within 120 s).

With `verify`, the engine requests the first byte of the URL with the browser's
request context (`aria2.probe`) before anything is created. With the confirm
behavior, or for `.torrent` links, the request stays open while the add dialog shows
the download; closing the dialog answers `cancelled`, and a request that times out
or whose extension disconnects is withdrawn from the dialog. Nothing is persisted:
an error or a lost answer leaves the download to the browser, so no download is
lost. Invalid input returns 400; an unavailable engine returns 503.

`GET /tasks` lists unfinished tasks in queue order and the ten latest finished
ones with the global statistics; `POST /tasks/{gid}/pause` and `…/resume` control
one task. These serve the extension popup.

## Native storage and recovery

`database/` is the single connection owner for `history.db`: history, birth records,
HTTP credentials, rename journal and BitTorrent selections. `schema.sql` defines the current
schema; `PRAGMA user_version` records it. Initialization runs from Rust startup.
Frontend stores expose named IPC operations and never execute SQL. Media
operations live in memory; their hidden engine task is the durable part.

SQLite transactions serialize history page/count queries.
History snapshots are unordered; the task store owns presentation sorting and reuses
the snapshot during periodic live refreshes. Only explicit invalidation reloads history.
Database browsing uses indexed default ordering with an ID tie-breaker for stable pages.
The current database layout is retained when opening existing data; unsupported
layouts fail explicitly. There is no legacy migration runner or automatic reset.
Only the user's explicit reset removes the database.

Saved HTTP passwords remain plaintext local data as before and are never URL decoded.

## Save locations and task files

Save locations use the current explicit choice, then the remembered explicit
choice (when enabled), then classification, then the default directory. Remembering
a location is opt-in and updates only after a successful user submission. Silent
browser submissions read the same preferences without changing the remembered path.

Task deletion resolves individual files from native snapshots and persisted history.
Other GIDs referencing the same path protect that file. No task operation infers
ownership of a whole directory or removes another record by content hash. A failed
file deletion retains its history record for retry.

After deleting files, native cleanup removes empty parent directories below the
task's save directory, deepest first. Download roots, category directories, shared
paths and nonempty directories remain. Cleanup does not traverse directory links;
permission failures retain the task record so cleanup can be retried.

Magnet selection policy is applied at native submission for every entry point.
SQLite retains the task's policy and Choose Later intent; the engine owns metadata
and selection readiness. Snapshots restore pending prompts after window recreation,
and the native monitor handles download-all classification without a WebView.
File selection and resume share the task mutation boundary and preserve a saved
selection when resume fails. User pause actions suspend automatic selection.

`services/tasks/files.rs` watches selected completed files through `notify` and
checks them periodically when native filesystem events are unavailable. Missing or
inaccessible seeding files pause sharing even without a WebView; resuming an affected
BitTorrent task requests libtorrent's recheck. Cards consume one shared availability
stream. A double-click opens completed content and ignores controls, drag handles
and selected text.

Start notifications belong to the native task lifecycle and honor `notifyOnStart`.
Submitting metadata or displaying an in-app toast never duplicates an OS notification.
Authenticated readiness endpoints return `engine_starting` or `engine_unavailable`
when the desktop is reachable but cannot provide engine services. Connect retains
the desktop version and offers recovery instead of repeatedly activating the app.

## Submission policy

`services::downloads::submit` is the single entry for dialog and browser tasks.
Site rules cap `max-connection-per-server` for matching hosts there: a plain
pattern names a domain and its subdomains, `*` matches any characters, and the
lowest matching limit wins over a larger requested value. The dialog's speed
limit and checksum become `max-download-limit` and `checksum`; a checksum names
one file, so it applies to a single non-magnet URI, and a mismatch fails the task.

Before submitting, the dialog asks the desktop for duplicates: live tasks with the
same source URI (ignoring fragments) and completed history records whose file still
exists. The user skips them, adds them anyway or returns to the form. Independently,
the engine never lets two unfinished tasks write one output path; the later task
takes a numbered name, or fails without `auto-file-renaming`.

An unfinished task's source can be replaced from task details. The desktop swaps
the URIs of single-file HTTP, FTP or SFTP tasks through `changeUri`, keeping the
downloaded data. A failed task is resubmitted with the new source instead.

Downloads handed over while the main window is hidden or minimized open the
standalone add-task window (`services/add_window.rs`) once the engine runs. It
holds the same dialog, receives later handoffs while open, and closes after
submission without raising the main window. Auto-submitted downloads use it
invisibly and close it when the engine accepted them.

Turning BitTorrent off (`bittorrentEnabled`) makes `submit` refuse magnet URIs
and torrents from every entry point. Independently, the engine runs with
`follow-torrent=false`: a `.torrent` fetched over HTTP is saved as a file and
never starts its content.

When a task completes, the optional post-download program runs detached with the
configured arguments. Arguments split at unquoted whitespace, double quotes group,
and `{file}`, `{dir}` and `{name}` are replaced after splitting, so paths with
spaces stay one argument. Its exit status is only logged.

## Snapshots and API limits

The pump's WebView snapshot carries at most 64 files per task, selected files
first, and sets `filesTotal` when it trimmed the list; the task whose details are
open (`tasks_set_detailed`) keeps every file. Native consumers read the whole
list. Tasks first seen after the initial snapshot get a start notification
whoever added them, including RPC clients and other extensions.

The extension API bounds work per client class: 64 reads and 512 writes in flight
(held confirmations keep an `/add` open), and a write budget of a 256-request
burst refilled at 32 per second. Excess requests get `429` with `Retry-After: 1`;
the extension then leaves the download to the browser.

## Verification and packaging

Use `pnpm test --maxWorkers=4`, `pnpm build`, `pnpm lint`, `pnpm check:repo`,
`pnpm format:check`, and `cargo test --workspace --all-targets`.
Native tests cover real SQLite transactions and local HTTP JSON-RPC responses,
including lost creation replies, structured errors, invalid UTF-8 and removal order.
Frontend tests cover transformations and visible state rather than forwarding wrappers.
The engine repository owns executable transfer tests; the extension owns browser API
and worker replay fixtures. Manual acceptance uses the three separately built apps.

Every packaged platform needs a matching engine artifact. In addition to the media
contract described in [MEDIA.md](MEDIA.md), it must advertise `filename-hints`.
Updating one platform's binary does not update the other bundled targets. Do not
publish a multi-platform desktop release with unmatched sidecars.

Prefer the established native APIs and maintained stable dependencies. Choose LTS
when that dependency provides a useful support window; do not invent an LTS version
for community libraries. File length is a review signal, not a reason to split one
cohesive algorithm. Comments explain ownership, units and non-obvious constraints;
tests protect behavior, data integrity and recovery rather than implementation shape.

## Product identity

The desktop advertises `product: "rayburst"` in `/ping`, download capabilities and
media capabilities. Rayburst Connect validates that field and sends
`X-Rayburst-Client: rayburst-connect` on authenticated requests. Browser-origin
requests without that header are rejected. Native clients without an Origin header
continue to authenticate with the Extension API secret.

The `rayburst://` scheme activates the desktop only. It never creates a download or
transports cookies. Downloads use the authenticated HTTP handoff.

## Activation and installation

The application repairs its own `rayburst` activation scheme at startup on Windows
and Linux. Public download schemes remain explicit user choices. Windows queries
the effective Shell association instead of inferring it from an application registry
key. Protocol state is included in the Windows diagnostic export.

All native wake requests use one serialized background window creation path.
WebView readiness controls delivery of pending UI actions; creating a window never
blocks a browser handoff answer. Non-silent automatic submissions honor the
Open Downloads preference.

The engine receives `stop-with-process` for the desktop PID. Windows installer hooks
run a temporary copy of the new executable in maintenance mode before replacing
files. The hooks replace Tauri's basename-only process check. Maintenance verifies
full process paths, stops this installation's supervisor before
its sidecars, and waits for native process handles to signal exit. It does not kill
other installations by executable name. Normal application exit still saves the
session through the existing supervisor.

Proxy bypass entries are validated and normalized once in Rust for runtime config,
global option updates and task options. libcurl performs host/IP/CIDR matching.
Aria2 Next resolves domain globs such as `*.example.com` and `dlinks.*` against each
request host before passing the result to libcurl. Redirects reevaluate that policy.
Newlines, commas and system-list semicolons separate entries. Trailing numeric
IPv4 wildcards become CIDR networks. System import reports unsupported expressions
for review before saving; `<local>` is not treated as an alias for localhost.

## Extension behavior

`extensionDownloadBehavior` defaults to `confirm`, allowing users to choose a
download directory. Explicit `background` and `show` preferences remain available. Native
dispatch applies that one policy; manual-task navigation stays independent. Closing
a confirmation finalizes every owned request ID, including URLs removed from the form.

## Platform integration

Windows installation registers the private `rayburst` scheme and advertises public
protocol/file candidates. It does not select defaults for magnet, ED2K, thunder or
torrent files. Existing user choices remain theirs; public defaults use OS settings.
The native messaging launcher starts the paired executable outside the browser's
Windows job with closed standard streams. macOS uses LaunchServices; Linux keeps a
separate process group. Development builds do not register installed-app identities.

Linux uses opaque window surfaces and a native StatusNotifierItem tray with direct
activation, speed tooltips and watcher reconnection. NVIDIA Wayland sessions disable driver explicit
sync for WebKit #324551 unless the environment already configures it. Other rendering
settings retain WebKitGTK defaults. `softwareRendering` explicitly enables a diagnostic
fallback; externally supplied rendering environment variables remain authoritative.
Linux diagnostics report the runtime WebKitGTK version, the existing WebView's native
hardware acceleration policy, and rendering overrides. A policy is not proof of GPU
use. An unavailable WebView or policy read failure is reported without creating a window.
Update policies are Stable and Beta (stable plus prerelease candidates). Only a
higher SemVer precedence is eligible; switching channels never offers a downgrade.
A later check supersedes an earlier pending result. Updates retain Tauri's selected Update object and verified bytes. Cancellation can
retry that selection, and installation needs no second metadata request.
