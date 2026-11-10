# Folders, Terminal, Editor and Git

The **Terminal**, **Editor** and **Git** tabs work on a folder you open. The chip in
the status row (a folder icon and the folder's name) shows which one, and opens a menu
to switch. Next to it, a git chip shows the branch and how many files have changed,
whichever tab you are on; clicking it opens the Git tab.

## Opening a folder

**Open folder…** shows the system folder dialog. Only a folder chosen there is added:
the app's own page cannot point these tabs at a path it names, because that page
renders model-written text and previews of generated pages. The last twelve folders
are remembered (`workspaces.json` in the [data folder](data-and-storage.md)); ✕ removes
one from the list and does not touch the folder.

The menu also lists **run sandboxes**: the folders REX's runs worked in, newest first,
labelled by when they were made. Open one to see what an autonomous run left behind.

Whole drives are refused, and a folder that has been moved or deleted is shown as
missing rather than failing later.

## Terminal

A real shell in the open folder, using your own shell and settings. It keeps running
when you switch tabs and its scrollback survives. If the shell exits, **Restart**
starts a new one.

The shell starts with an allowlisted environment: what it needs to work (`HOME`,
`PATH`, locale, the Windows system variables, your SSH agent socket) and nothing else,
so an API key or token set in the app's own environment never reaches it.

## Editor

Every file in the folder (inside a git repository, what git tracks plus what it does
not ignore, so `node_modules` and build output stay out of the way), with a filter and
**+** to create a file. It uses Monaco, bundled with the app, so it works offline.

- **Ctrl+S** saves, straight to the file. It is your own edit, so there is no
  approval step. The write is atomic and keeps the file's permissions.
- Switching files, folders, or closing the window with unsaved edits asks first.
- Files over 2 MB, binary files and files that are not UTF-8 are not opened, with the
  reason, because saving them from a text editor could corrupt them.

## Git

Your own `git` with your own configuration, acting on your own repository; the commit
message is yours and nothing goes through the model approval gate.

- Branch, head, upstream and ahead/behind.
- Files grouped as **Conflicts**, **Staged**, **Changes** and **Untracked**, each with
  a stage or unstage button, and *Stage all* / *Unstage all*. A file edited after
  staging appears in both lists.
- Click a file for its diff. A file git has not seen is shown as wholly added.
- Write a message and **Commit** (or **Ctrl+Enter**). If git does not know who you are
  yet, the error tells you the two commands that fix it.
- A folder that is not a repository offers **Initialize repository**.
- Recent commits, and **Checkpoints** (below).

Status is read about every five seconds while the window is in view and when it regains
focus, without taking git's index lock from under your terminal.

### Checkpoints

A snapshot of the folder you can restore with one click; restoring first saves the
current state so it can be undone. A snapshot covers your project's files and **not**
`node_modules`, `target`, `dist`, `build` and similar folders, which are neither copied
nor deleted. Symlinks are never followed. A folder over 20,000 files or 256 MB is
refused rather than copied.
