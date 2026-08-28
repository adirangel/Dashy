# Dashy

Dashy is a compact, local-first desktop productivity dashboard with a Hebrew right-to-left interface.

The application uses React and TypeScript for the user interface, Vite for the development environment, and Tauri 2 with Rust for the Windows desktop shell.

## What We Built

- A compact, transparent, frameless dashboard window that stays above other windows.
- An activity streak display and weekly activity heatmap.
- Sample usage cards for Claude and ChatGPT.
- A task list that lets users add tasks and mark them as completed.
- A progress indicator based on the number of completed tasks.
- A responsive design optimized for Hebrew and window widths of 320 pixels or more.
- Unit tests for the activity streak calculation.

> [!NOTE]
> This is still a prototype. Tasks and usage data are currently stored in memory and reset when the page is refreshed or the application is closed. The close and settings buttons are visual only, and there is no live integration with Claude or ChatGPT yet.

## Project Structure

```text
Dashy/
├── frontend/        React, TypeScript, Vite, and styling
├── backend/         Tauri desktop shell and Rust code
├── agents/          Planned boundaries for future automation
├── infrastructure/  Future packaging and delivery configuration
└── package.json     Shared development, test, and build commands
```

## Prerequisites

### Browser Preview

- [Node.js LTS](https://nodejs.org/) and npm.

### Windows Desktop Application

In addition to Node.js, install:

- [Rust through rustup](https://www.rust-lang.org/tools/install) with an MSVC toolchain.
- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++** selected.
- Microsoft Edge WebView2. It is normally preinstalled on Windows 10 version 1803 or later and on Windows 11.
- Tauri CLI version 2.

See the [official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for the complete and most current requirements.

## Installation

Open PowerShell and run:

```powershell
Set-Location D:\dev\Dashy
npm ci --prefix frontend
```

`npm ci` installs the exact package versions recorded in `frontend/package-lock.json`.

## Run in a Browser

This is the quickest way to preview and work on the interface without installing Rust:

```powershell
Set-Location D:\dev\Dashy
npm run dev
```

Then open [http://localhost:5173](http://localhost:5173) in a browser. Code changes are loaded automatically.

Press `Ctrl+C` in PowerShell to stop the development server.

## Run as a Desktop Application

Install the Tauri CLI once:

```powershell
cargo install tauri-cli --version "^2.0.0" --locked
```

Then run the application from the project:

```powershell
Set-Location D:\dev\Dashy\backend
cargo tauri dev
```

Tauri starts the Vite server, compiles the Rust code, and opens the Dashy window. The first run may take several minutes while Cargo downloads and compiles dependencies.

## Tests and Builds

Run the unit tests:

```powershell
Set-Location D:\dev\Dashy
npm test
```

Build the web interface:

```powershell
npm run build
```

The output is written to `frontend/dist`.

Build the Windows application and installer packages:

```powershell
Set-Location D:\dev\Dashy\backend
cargo tauri build
```

The installer files are created under `backend/target/release/bundle`. Building an MSI installer may require the optional Windows VBSCRIPT feature to be enabled.

## Troubleshooting

- **`npm` is not recognized** — install Node.js LTS and open a new PowerShell window.
- **`cargo` or `rustc` is not recognized** — install Rust through rustup and open a new PowerShell window.
- **Linker or `link.exe` error** — confirm that Microsoft C++ Build Tools is installed with **Desktop development with C++**.
- **The application window is blank** — confirm that WebView2 is installed and that port `5173` is available.
- **MSI build error** — enable the Windows Optional Feature named VBSCRIPT and try again.

## Useful Commands

| Command | Result |
|---|---|
| `npm run dev` | Starts the browser interface with hot reload |
| `npm test` | Runs the Vitest test suite |
| `npm run build` | Type-checks and builds the web interface |
| `cargo tauri dev` | Starts the desktop application in development mode |
| `cargo tauri build` | Builds the application and installer packages |
