# COSMIC Numbered Workspaces with application icons

This is an experimental fork of
[pop-os/cosmic-applets](https://github.com/pop-os/cosmic-applets) that extends
the **Numbered Workspaces** applet with application icons.

## Why?

One of the reasons I switched to COSMIC Desktop is that it can manage
workspaces independently on each external monitor, similarly to macOS.
However, with several monitors and workspaces, it can sometimes be difficult
to remember where each application window is located.

The Numbered Workspaces applet already does a good job of highlighting the
currently active workspace. This fork adds the icons of the applications on
each workspace, making the applet into a compact map of the desktop.

At a glance, it is possible to see which monitor and workspace contain each
open application. Hovering over a workspace shows a tooltip with additional
details, and applications whose windows are all minimized are visually
dimmed.

## Features

- Displays application icons beside each workspace number.
- Associates windows with the correct workspace and monitor.
- Shows up to five application icons before displaying a `+N` overflow count.
- Groups the workspace number and its application icons into one visual pill.
- Lists applications and useful window titles in a tooltip.
- Dims an application's icon when all of its windows are minimized.
- Preserves the original workspace switching, scrolling, highlighting, and
  workspace-overview behavior.

## Requirements

- COSMIC Desktop with the original Numbered Workspaces applet installed.
- Git.
- A Rust toolchain with Cargo.
- The development libraries required to build COSMIC applets.

The installation helper expects the original desktop entry to exist at:

```text
/usr/share/applications/com.system76.CosmicAppletWorkspaces.desktop
```

## Installation

Clone this fork and select the feature branch:

```bash
git clone \
  --branch feature/workspace-app-icons \
  https://github.com/crocodile/cosmic-applets.git

cd cosmic-applets
```

Build and activate the modified applet:

```bash
./local-workspaces-applet.sh install
```

The script:

1. Builds `cosmic-applet-workspaces` in release mode.
2. Copies the system desktop entry into
   `~/.local/share/applications/`.
3. Changes the copied desktop entry to launch the locally built binary.

The helper does not replace files in `/usr/bin`. Keep the cloned repository in
a permanent location because the desktop entry points directly to its
`target/release/cosmic-applet-workspaces` binary. If the repository is moved,
run `install` again from its new location.

After installation, open the COSMIC panel settings, remove **Numbered
Workspaces**, and add it again. If COSMIC still launches the packaged version,
log out and back in once.

## Helper script commands

```bash
# Build and activate the local applet
./local-workspaces-applet.sh install

# Rebuild without changing the desktop entry
./local-workspaces-applet.sh build

# Show which executable the desktop entry currently launches
./local-workspaces-applet.sh status

# Remove the local override and return to the packaged applet
./local-workspaces-applet.sh uninstall
```

After uninstalling, remove and re-add Numbered Workspaces in the panel
settings.

## Updating and reinstalling

The local desktop entry launches the binary in this repository's
`target/release/` directory. Updating the source does not automatically
rebuild that binary.

After pulling changes to this fork, run the installer again:

```bash
git pull --ff-only
./local-workspaces-applet.sh install
```

You should also rebuild and reinstall after updating the original COSMIC
applets package. A system update will normally leave the user-local desktop
entry in place, which means COSMIC may continue launching an older locally
built binary. Running `install` again rebuilds the applet and refreshes the
desktop entry copied from the system package.

Remove and re-add the applet after reinstalling so the panel starts the new
binary.

## Rebasing onto upstream

The upstream repository may change the workspace applet or its COSMIC
dependencies. This branch may therefore need to be rebased periodically.

Add the upstream repository once:

```bash
git remote add upstream https://github.com/pop-os/cosmic-applets.git
```

Before rebasing, commit or stash any local changes. Then run:

```bash
git fetch upstream
git switch feature/workspace-app-icons
git rebase upstream/master
```

If conflicts occur:

```bash
# Edit and resolve the conflicting files
git add <resolved-files>
git rebase --continue
```

After the rebase, rebuild and reinstall:

```bash
./local-workspaces-applet.sh install
```

Because rebasing rewrites commit history, updating an existing remote feature
branch may require:

```bash
git push --force-with-lease origin feature/workspace-app-icons
```

## Development checks

```bash
cargo fmt --all -- --check
cargo check -p cosmic-applet-workspaces
cargo test -p cosmic-applet-workspaces
cargo clippy -p cosmic-applet-workspaces -- -D warnings
```

The modified applet source is in:

```text
cosmic-applet-workspaces/
```

The helper script is:

```text
local-workspaces-applet.sh
```
