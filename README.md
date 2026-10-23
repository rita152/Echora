<h1 align="center">Echora</h1>

<p align="center"><strong>A native desktop workspace for working with Codex.</strong></p>
<p align="center">Rust + GPUI · Powered by Codex app-server</p>
<p align="center"><strong>English</strong> · <a href="README.zh-CN.md">简体中文</a></p>

Echora brings conversations, local projects, files, terminals, web browsing, and code review into one desktop application. Its interface draws on the ChatGPT app, with a conversation at the center and the tools you need alongside it.

Echora is an independent application in early development. It currently connects to Codex through the locally installed Codex CLI.

![Echora workspace in dark mode](docs/images/workspace-dark.png)

<details>
<summary>Light theme</summary>

![Echora workspace in light mode](docs/images/workspace-light.png)

</details>

*Screenshots were captured from the current native build with example conversations, files, and server inventories. Some interface labels still use the Codex name.*

## Work with your agent

Start a conversation in a local project, attach context, and follow the agent's replies and tool activity as they arrive. Plans, commands, searches, and file changes appear in the conversation, while approval cards let you inspect actions that need permission.

- Add files, folders, images, skills, plugins, local chats, and pages from the in-app browser as context. Drop files or macOS screenshot thumbnails anywhere in the conversation to attach them; unsaved screenshots are received locally and shown as image previews. Image previews fit the available window area without a bottom zoom bar. Open an unsent image attachment and choose **Mark up** to add strokes, shapes, or text, with erasing and undo/redo. Adjust stroke width by clicking or dragging the left slider, scrolling over it, or focusing it and using the arrow keys; saving replaces that draft attachment at its original image size without changing the source file.
- Choose the permission options exposed by your Codex configuration. New chats remember your last confirmed choice; permission options refresh automatically after a backend reconnect.
- Send new instructions while the agent works, or queue follow-ups for later.
- Return to an unsent draft with **New chat** in the same project or section. Its prompt and attachments stay in memory while switching chats; titlebar Back/Forward also restores the original conversation.
- Use plan mode, set a goal, or start a code review from the slash menu when supported by the backend.
- Stop a turn, resume an interrupted conversation, search its messages, and jump between prompts.
- Follow Computer Use in a stack of live native windows in the conversation’s upper-right corner. On macOS, ScreenCaptureKit streams up to five matching windows directly to GPU surfaces; previews keep updating between tool calls. Click a rear preview to bring it to the front. Click the front preview to stop the agent, wait for confirmation, and focus that exact window. Collapsing the stack, leaving the chat, or ending the turn releases capture streams. Missing Screen Recording or Accessibility access is reported without prompting; any fallback tool image is explicitly labeled as a tool capture. Browser Use renders the controlled tab screenshot supplied by the tool in an aspect-preserving preview of up to 400 × 400 pt, updates when a new screenshot arrives, and hides when the turn ends. It does not start native browser-window capture.
- Open a temporary side chat to explore a separate question alongside the main conversation.

## Keep projects and conversations organized

Create local projects from existing folders, or create a new Git workspace. Choose a project and local checkout or new worktree before sending the first message.

The sidebar keeps projects and conversations within reach. Search, rename, pin, archive, delete, or move chats, and organize them into custom sections. The activity view brings together running chats, requests waiting for your attention, and unread results. Switching chats keeps background work running. The bell badge reflects pending requests or unread results; opening a chat or marking it read clears the unread reminder, including failed results.

## Use tools beside the conversation

The right panel holds multiple tabs, so you can keep a file, terminal, browser page, and diff open together. Drag tabs to reorder them or expand the selected tool for more space.

| Tool | What you can do |
|---|---|
| **Files** | Browse and search local files, edit text with autosave, preview Markdown and images, and follow file links to a specific line. Markdown previews render HTML headings, aligned paragraphs, emphasis, links, and expandable details; relative links and images resolve beside the document. Scripts and embedded applications do not run. |
| **Terminal** | Run a local shell in the conversation's working directory, with multiple tabs, scrollback, selection, and clipboard support. |
| **Browser** | Browse pages in the embedded WebKit browser, use address and history suggestions, search within a page, manage downloads, and attach page content to a chat. |
| **Changes** | Available in Git working directories; switching to a non-Git directory removes the Changes tab. Review Git diffs in unified or split view, add line comments, stage or unstage changes, revert files or hunks, commit, create branches, push, and create pull requests. |
| **Side chat** | Explore a question in a temporary conversation with its own input, permissions, and stop control. |

The Environment panel provides quick access to the working location, branches, Git actions, attached pull requests, local servers, and background terminals. The separate Pull requests page opens a Code review inbox for GitHub issues and pull requests in the current repository (or items involving your account outside a repository). Search or paste a GitHub item link, switch between All / PRs / Issues, filter by status, involvement, project or labels, sort by creation or update time, and pin items locally. Summary / Changes / Timeline preserve descriptions, Markdown, comments, checks, commits, diffs and code review, with assignees, labels and other metadata in a separate details column. The action bar provides pin, copy link, GitHub, Send to agent and more actions; issue views omit PR-only operations.

<details>
<summary>See the file editor and Git changes</summary>

**Local file browsing and editing**

![Local file tree and Rust file editor beside a conversation](docs/images/product-files.png)

**Git diff review**

![Git changes panel showing a diff from the example project](docs/images/product-changes.png)

</details>

## Make it your workspace

Echora supports light and dark themes, plus English, Simplified Chinese, and automatic language detection. Change the interface language in **Settings → General → Language**.

On macOS, **Settings → General → Keep Echora running after closing the window** controls the red close button. It is off by default, so closing quits. When enabled, closing hides the window while chats, drafts, panels, and running tasks stay alive; click the Dock icon to continue. **Quit Echora** and **⌘Q** always quit, preserving the existing unsaved-file confirmation.

**Settings → Appearance** also offers independent UI (14 px by default, 11–16) and code (12 px, 8–24) font sizes. Enter or leave a field to apply and save it locally. UI size controls prose, inline code and command output; code size controls fenced code, files, terminals and diff text/line numbers. Existing views update immediately. Out-of-range or invalid entries restore the previous value; in-range fractions are accepted.

The sidebar **Plugins** entry opens a dedicated directory with All and Installed views, live search, entries grouped by their actual marketplace, plugin details, and confirmed install/uninstall actions. All independently reads local and remote catalog types, retains successful sources when another requires ChatGPT sign-in or fails, and silently skips empty or unavailable catalog sources. Installed reads its own inventory without waiting for catalogs. Directory search filters the complete loaded catalog union or installed inventory locally. The Skills tab reads the agent’s available skills and renders their declared PNG/SVG artwork, with theme-aware defaults when artwork is absent or cannot load. Catalogs, declared artwork, permissions, loading states, and errors come from Codex app-server; entries without artwork use the reference app’s stable default logos; ChatGPT-only catalogs and private services are not substituted. App-managed `codex-app-tools` and `unified-computer-use` entries from `openai-bundled` are hidden in directory views, matching the reference app. Returning to a chat restores its draft and panels.

Settings also provides access to supported Codex preferences, skills, plugins, MCP server status and OAuth login, hooks, experimental features, and memory controls. Available features depend on the connected account, Codex version, and configuration.

The sidebar **Automations** entry opens the Tasks board. Kanban offers project overview and project columns, Attention / Classic layouts, and a Needs review filter for open pull requests. Each column scrolls independently to its last card and Show more control while its heading stays visible; narrow project boards also scroll horizontally. Create a task with the chat editor's files, skills, images, model, effort, permissions, Plan mode, and Local / Worktree options. **Send as draft** saves it locally; click a saved draft to reopen this editor, or drag it into **In Progress** to execute it through the agent. Cards open the real conversation and reflect running, approval/input, failure, stuck, and completed states. Their menus support rename, pin, copy path/thread ID, archive, goals, and confirmed deletion. **⌥⌘T** opens New task; **⌥↑/↓** reorders drafts.

**List** manages separate local todos with notes, due dates, priorities, completion, and optional project/chat links. Press Enter to add one or Tab to hand it to an agent, then configure its task editor. Linked details provide the latest reply, reply input, Stop, and Open chat. View choices, draft input/options/order, and todos survive restart in Echora's UI preferences. Voice transcription and Debug mode are explicitly unavailable with the current agent; system dictation can enter text. **Set as goal** saves a paused goal for explicit resumption in chat because the current protocol lacks Synara's deferred-start option. These are capability boundaries, not simulated actions.

<details>
<summary>See MCP and skills management</summary>

**MCP server status**

![MCP management page with example server connection states](docs/images/product-mcp.png)

**Skills inventory**

![Skills management page with example skills and enable controls](docs/images/product-skills.png)

</details>

## Get started

The current development and interactive verification platform is **macOS**. To build and run the app, install:

- The Rust toolchain specified in [rust-toolchain.toml](rust-toolchain.toml), currently **1.97.1**.
- Xcode and its **Metal Toolchain** component, with Xcode selected as the active developer directory.
- Codex CLI, configured and ready to use. The current integration baseline is **codex-cli 0.158.0**. On macOS/Linux, Echora first searches its inherited `PATH`, then the user's interactive login shell (up to five seconds), then common installation directories (including NVM versions, preferring its numeric default). Finder launches can therefore discover NVM-managed installations without a wrapper script. Only the child tool `PATH` is supplemented; shell credentials and other environment changes are not imported. Windows uses the inherited `PATH`.
- Git for repository features, and an authenticated GitHub CLI (`gh`) for pull request features. GitHub CLI uses the same desktop executable discovery as Codex, including login-shell PATH and Homebrew directories, for listing, reviewing, and creating pull requests. If it cannot be found, install it, run `gh --version` and `gh auth login` in a terminal, then click **Try again**.

```bash
git clone https://github.com/rita152/Echora.git
cd Echora
cargo run --locked --release -- --theme=dark
```

If the Metal compiler is missing, install and verify it before building:

```bash
xcodebuild -downloadComponent MetalToolchain
xcrun --sdk macosx metal --version
```

Use `--theme=light` for the light theme. Add `--language=en`, `--language=zh-CN`, or `--language=auto` to choose the interface language for one launch. Settings saves your language preference for future launches.

`--print-diagnostics` reports the resolved Codex executable and discovery source, or an actionable discovery error. To check desktop launch discovery from a terminal on macOS, run `env PATH=/usr/bin:/bin:/usr/sbin:/sbin /Applications/Echora.app/Contents/MacOS/gpui-chat-clone --print-diagnostics`.

Download development previews from [GitHub Releases](https://github.com/rita152/Echora/releases):

| Platform | Installer | Installation |
| --- | --- | --- |
| macOS Apple Silicon (arm64) | `Echora-<version>-macos-arm64.dmg` | Open the disk image and drag Echora into Applications. |
| Windows x86_64 | `Echora-<version>-windows-x86_64-setup.exe` | Run Setup. Installs for the current user with a Start menu entry and an uninstaller. |
| Linux x86_64, Ubuntu 24.04 or a compatible newer distribution | `Echora-<version>-linux-amd64.deb` | Open with your package manager, or run `sudo apt install ./Echora-<version>-linux-amd64.deb`. Launch Echora from the application menu. |

Installers include application assets; Windows also includes the app-local MSVC runtime. Codex CLI must still be installed and configured separately. Development installers do not have trusted code signatures or macOS notarization, so the operating system may require approval before opening them. Interactive verification is currently limited to macOS; CI checks package installation, resource discovery, and removal on the corresponding runners, without testing the full Windows/Linux GUI.

The `.tar.gz` downloads are portable executable-and-assets archives, not installers; keep the executable beside its `assets/` directory. The executable is currently named `gpui-chat-clone`. `SHA256SUMS` covers all installers and portable archives. GitHub's **Source code** downloads contain source, not an installed app. Main-branch and pull-request builds remain available for 14 days on the [Actions page](https://github.com/rita152/Echora/actions/workflows/release.yml).

Performance diagnostics are opt-in: set `ECHORA_PERF_LOG=/tmp/echora-frames.jsonl` when launching Echora, then run `python3 scripts/summarize_performance.py /tmp/echora-frames.jsonl`. The JSONL records CPU frame phases, viewport/DPR, scroll/resize delivery to draw completion and file-editor preparation, and invalidation/rebuild sources without chat or patch content. On macOS, Metal's drawable `presentedTime` separately records display timestamps and event-delivery-to-display latency; unavailable timestamps are counted and excluded. Display intervals include idle gaps, so neither they nor CPU submission times alone establish animation FPS. Input-device latency and time in the OS event queue are excluded. `--print-diagnostics` includes the build commit and a dirty-tree marker.

For a repeatable macOS workload, run `scripts/run_native_verification.sh performance --check-permissions` first. On a dedicated capture instance, pass `--app='exact instance name' --capture=/absolute/app-capture.png --perf-log=/tmp/echora-frames.jsonl --output=artifacts/performance --reset-top --seconds=16`. The driver sends scroll input at 60 Hz and preserves original before/after renderer captures and measurement boundaries; summarize with `python3 scripts/summarize_performance.py /tmp/echora-frames.jsonl --workload artifacts/performance/workload.jsonl`. Use `--scenario=idle --seconds=60` for an idle interval, or `--scenario=soak --seconds=7200` for alternating conversation scrolling, sidebar scrolling and idle periods. Package with `GPUI_CAPTURE_PROFILE=release scripts/package_gpui_capture.sh`; use the same optimized binary for cold/aged and diagnostics-on/off comparisons. Keep data, theme, viewport, DPR and disclosure state fixed. The driver stops if its dedicated app loses foreground. Sample idle CPU separately with `python3 scripts/measure_process_usage.py --pid=PID --seconds=60 --output=artifacts/performance/usage.json` (100% means one logical core).

For a repeatable native divider drag, run `scripts/run_native_verification.sh resize --check-permissions` first, then `scripts/run_native_verification.sh resize --app="<verification bundle name>" --start-x=750 --end-x=1080 --y=420 --seconds=2 --cycles=4 --capture="<absolute app capture path>" --output=artifacts/resize`. Package and launch the current worktree with `scripts/package_gpui_capture.sh` and `scripts/launch_verify_instance.sh`; use the actual divider position and the capture path printed by the launcher. The driver writes input timings and preserves the app-owned PNG and render metadata.

Titlebar zoom/restore uses `scripts/run_native_verification.sh zoom --check-permissions`, followed by `scripts/run_native_verification.sh zoom --app="<verification bundle name>" --cycles=2 --seconds=2 --capture="<absolute app capture path>" --output=artifacts/zoom`. It records each double-click and window geometry during the animation, checks that restore recovers the original frame, and preserves the app-owned capture.

### Publishing a version

Versions follow SemVer: `v0.1.0-alpha.1`, `v0.1.0-alpha.2`, then beta/rc previews as needed, and `v0.1.0` for a stable release. Any version with a prerelease suffix is automatically marked **Pre-release** on GitHub and is not marked Latest.

Update the root package version in `Cargo.toml` and its entry in `Cargo.lock`, commit and push the change, then push the matching `v` tag (for example, `git tag v0.1.0-alpha.2` followed by `git push origin v0.1.0-alpha.2`). The workflow validates the version, builds all three platforms, packages and checks native installers, and creates a GitHub Release with all downloads and checksums only after every job succeeds. Publishing a release through GitHub's UI also triggers builds and attaches the same assets; its tag must match the version in the tagged source. To retry a failed release build, rerun the workflow. To rebuild an existing release with updated packaging tools, manually dispatch the workflow from `main` with `release_tag` set to the existing tag (for example, `gh workflow run release.yml --ref main -f release_tag=v0.1.0-alpha.1`). Application source and assets come from that tag; packaging tools come from the workflow commit. This updates release assets without moving the tag.

## Current scope

- **Codex is the only integrated agent.** Echora starts the local Codex app-server and uses its configuration and conversation history.
- **The ChatGPT interaction experience is only partly implemented.** Some navigation and settings entries remain placeholders; scheduled tasks, remote hosts, and ChatGPT cloud workflows are not connected.
- **The browser is for browsing and sharing page context.** Agent control of the embedded browser and Chrome extensions are not supported.
- **Some state is temporary.** Terminal sessions and side chats end when the app exits. Activity indicators reflect turns observed by Echora's own connection; work running in another client may not appear as active here.

Echora is not an official OpenAI product. Detailed backend coverage and compatibility notes are maintained in the [Codex app-server integration reference](docs/APP_SERVER_INTEGRATION.md).
