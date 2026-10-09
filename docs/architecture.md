# Architecture

Fast prose linter in Clojure. Modular, functional, data-driven.

## Design principles

1. **Separation**: Commands describe, effects execute
2. **Extensibility**: Checks are data, registry is pluggable
3. **Speed**: Parallel processing, smart caching, hash-based detection
4. **Safety**: Result monad everywhere
5. **UX**: Numbered issues, contextual ignores, consistent output

## How it works

### Command-effect pattern

```
CLI Options → Commands → Effects → Results
```

- **commands.clj**: Pure handlers (just data)
- **effects.clj**: Multimethod dispatcher (does I/O)
- Logic stays separate from side effects, easy to test

### Processing pipeline

```
Options → Validate → Input → Vet (cached) → Output
```

1. **Validate** ([commands.clj:344](../src/proserunner/commands.clj)): Check for flag conflicts
2. **Input** ([vet/input.clj](../src/proserunner/vet/input.clj)): Validate file, load config, read lines
3. **Vet** ([vet.clj](../src/proserunner/vet.clj)): Run checks, use cache
4. **Output** ([output.clj](../src/proserunner/output.clj)): Format and display

**Key data structures:**

- `Issue` - Single finding (file, line, col, specimen, message)
- `Line` - Text line with issues attached
- `Result` - Success/Failure wrapper
- `Config` - Merged config with enabled checks

**Performance** - two strategies (pick one):

- Default: parallel files, sequential lines (best for many files)
- `--parallel-lines --sequential-files`: parallel lines, sequential files (best for one big file)

## Main systems

### Checks (editors)

Plugin system via registry:

- **Registry** ([editors/registry.clj](../src/editors/registry.clj)): Dispatches to check types
- **Check types**:
  - Standard (existence, recommender, case)
  - Regex (pattern matching)
  - Repetition (catches "the the")
- **Utilities** ([editors/utilities.clj](../src/editors/utilities.clj)): Shared detection code

Checks are EDN files loaded from config dirs:

```clojure
{:name "corporate-speak"
 :kind "existence"
 :specimens ["utilize" "leverage" "synergy"]
 :message "Corporate speak detected"
 :case-sensitive false}
```

See [default checks repo](https://github.com/jeff-bruemmer/proserunner-default-checks).

### Config

Layered system (project overrides global):

1. **Global** (`~/.proserunner/`): Personal defaults
2. **Project** (`.proserunner/`): Repo settings
3. **Manifest** ([config/manifest.clj](../src/proserunner/config/manifest.clj)): Finds project config
4. **Loader** ([config/loader.clj](../src/proserunner/config/loader.clj)): Parses EDN
5. **Merger** ([config/merger.clj](../src/proserunner/config/merger.clj)): Combines layers

First run downloads default checks from GitHub.

**Storage:**

- `~/.proserunner/default/` - Default checks (downloaded on first run)
- `~/.proserunner/custom/` - Your global custom checks
- `~/.proserunner/config.edn` - Global config
- `~/.proserunner/ignore.edn` - Global ignores
- `.proserunner/config.edn` - Project config, including project ignores (commit this)
- `.proserunner/checks/` - Project checks
- `$XDG_CACHE_HOME/proserunner/` - Cache (see [usage](usage.md#cache) for the full lookup order)

**Smart defaults**: `--ignore-issues` uses project scope if `.proserunner/` exists, else global. Override with `--global` or `--project`.

### Ignores

Run it, fix what matters, ignore the rest.

Context-aware system:

- **Core** ([ignore/core.clj](../src/proserunner/ignore/core.clj)): Filters issues
- **Context** ([ignore/context.clj](../src/proserunner/ignore/context.clj)): Resolves scope
- **File** ([ignore/file.clj](../src/proserunner/ignore/file.clj)): Saves to disk
- **Audit** ([ignore/audit.clj](../src/proserunner/ignore/audit.clj)): Finds stale entries

Three types:

- **Specimens**: Ignore "hopefully" everywhere (disagree with check)
- **Contextual**: Ignore at file:line:col (intentional here)
- **Issue numbers**: Ignore [1], [3], [5] (quick - pick from output)

Ignores remember context. Only suppress exact issue you saw. Move text, ignore stays at old location. Keeps list clean.

**Issue numbering**: Numbers are sequential display IDs for the current run only. Not preserved across runs. When you ignore issue [56], the system stores the location (file:line:col:specimen), not the number. Next run renumbers remaining issues sequentially.

### Error handling

Result monad everywhere ([result.clj](../src/proserunner/result.clj)):

- `result/ok` - wraps success
- `result/err` - wraps failure with context
- `result/bind` - chains operations
- `try-result-with-context` - safe execution

Every effect returns a Result. Errors propagate cleanly.

## Module Organization

```
src/
├── editors/              # Check implementation (pluggable)
│   ├── registry.clj      # Dynamic dispatch
│   ├── utilities.clj     # Shared check logic
│   ├── re.clj            # Regex-based checks
│   └── repetition.clj    # Repetition detection
│
└── proserunner/
    ├── commands.clj      # Command handlers (pure)
    ├── effects.clj       # Effect execution (I/O)
    ├── process.clj       # Main orchestration
    ├── vet.clj           # Check execution coordinator
    │
    ├── config/           # Configuration management
    │   ├── manifest.clj  # Project discovery
    │   ├── loader.clj    # EDN parsing
    │   └── merger.clj    # Config composition
    │
    ├── ignore/           # Ignore system
    │   ├── core.clj      # Filtering logic
    │   ├── context.clj   # Scope resolution
    │   ├── file.clj      # Persistence
    │   └── audit.clj     # Cleanup tools
    │
    ├── output/           # Output formatting
    │   ├── format.clj    # Multi-format output
    │   ├── prep.clj      # Issue preparation
    │   └── checks.clj    # Check listing
    │
    └── vet/              # Vetting subsystem
        ├── input.clj     # Input validation
        ├── processor.clj # Parallel execution
        └── cache.clj     # Result caching
```

## Developer guide

### Entry point

Starts in [core.clj](../src/proserunner/core.clj) `-main`:

1. Parse CLI args (clojure.tools.cli)
2. Validate options ([commands.clj:344](../src/proserunner/commands.clj))
3. Dispatch to command handler
4. Execute effects
5. Handle Results, print output

### Adding commands

Follow command-effect pattern:

1. **Add option** to `core.clj` options vector
2. **Create handler** in `commands.clj`:
   ```clojure
   (defn handle-my-command [opts]
     {:effects [[:my-effect/do-thing opts]]
      :messages ["Doing the thing..."]})
   ```
3. **Register handler** in `commands.clj` handlers map
4. **Add to dispatch** in `determine-command`
5. **Implement effect** in `effects.clj`:
   ```clojure
   (defmethod execute-effect :my-effect/do-thing
     [[_ opts]]
     (effect-wrapper
       #(do-the-thing opts)
       :my-effect/do-thing
       {:opts opts}))
   ```

Handlers return `{:effects [...] :messages [...]}`. Effects return Results.

### Cache system

Hash-based invalidation ([vet/cache.clj](../src/proserunner/vet/cache.clj)):

- **File hash**: Content-based (`stable-hash`)
- **Config hash**: Enabled checks, settings
- **Checks hash**: Check definitions

Cache hits need all three hashes to match. Partial match (checks same, lines changed) = incremental update. Only processes changed lines, reuses cached results.

Cache lives at `$XDG_CACHE_HOME/proserunner/` by default; see [usage](usage.md#cache) for the full lookup order.

### Parallelism

Two strategies ([vet/processor.clj](../src/proserunner/vet/processor.clj)):

- **Parallel lines** (off by default): `pmap` lines concurrently
- **Parallel files** (on by default): Higher-level, multiple files

Mutually exclusive (validated in `commands.clj`). Uses Clojure `pmap`.

### Testing

Tests in `test/proserunner/`:

```bash
bb test              # All tests
bb test-build        # Build verification
```

Or: `clojure -M:test`

See [installation.md](installation.md) for build info.
