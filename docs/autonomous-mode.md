# Autonomous mode

By default REX stops and asks before each file write and each command. **Autonomous
mode** is the choice, made once when you start a run, not to be asked. REX then works
on its own until it finishes, reaches a limit, or you stop it.

It is in the desktop app: the run-mode control under the task box reads
**Build · Plan · Auto**. The first time in a session you choose **Auto**, REX shows
what you are agreeing to and asks you to confirm; **Keep asking me** is the default
button, so a stray key never turns approvals off. While it is on, an amber
*Autonomous · no approval prompts* chip sits in the status row, and a run started in
it says so at the top of its view.

## What changes

- **No approval prompts** for file writes, patches and commands that REX's tool
  runtime has already accepted.
- **Questions are declined.** If the model would have asked you something, REX tells
  it to proceed on its own judgement and say what it assumed.
- **Each action is still recorded.** Every call that ran without a prompt appears in
  the run's activity as *Ran without asking · …*, and the run header counts them. The
  choice itself is written into the run's immutable brief, so a resumed run keeps it.

## What does not change

Autonomous mode answers "ask me first?" ahead of time. It grants nothing the run did
not already have:

- A call the tool runtime refuses is still refused: paths outside the sandbox,
  symlink escapes, shells, interpreters, network tools and destructive programs.
  Those never reach the point where an approval would be asked.
- The run's capability grant and budgets (steps, tool calls, tokens, time) still end
  it.
- **Cancel** still ends it at once.
- The run still works in its own sandbox folder under the data folder. Nothing is
  written to your project until you do that yourself.
- Completion is still decided by the evidence gates, not by the model's say-so.

## The exceptions

- **Calls into external MCP servers still ask.** REX cannot see what those tools do,
  so it will not approve them for you.
- **It cannot be combined with Plan.** A plan gate is a human decision, and
  autonomous mode is the choice not to make them. Choosing one turns the other off.
- **Only a human can turn it on.** A run started by an agent operator (for example
  through `rex-mcp`) that asks for it is refused: an agent must not approve its own
  work.
- It applies to the default **custody** run. The installed-agent, Ultra and
  delegated runs have their own gates, and the **Auto** option is disabled for them.

## It is not remembered

You confirm once per session, and it is never saved across restarts. It stays on for
the session until you choose **Build** or **Plan**, so a series of tasks does not
need a confirmation each. In the command line the equivalent is `--yes`, which is
for CI and sandboxes ([security.md](security.md)).

## When to use it

When the task is bounded, the run's sandbox is all it can touch, and you would rather
read the result than watch each step. When the cost of a wrong action is high, or you
want to see what a command is before it runs, stay on Build.
