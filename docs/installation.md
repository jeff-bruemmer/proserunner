# Installation

## Native binary (fastest)

Builds a ~65MB standalone binary with ~50ms startup:

```bash
git clone https://github.com/jeff-bruemmer/proserunner.git
cd proserunner
bb install  # Builds and installs to ~/.local/bin
```

**You'll need:**

- Babashka
- GraalVM 25+ with native-image
- Clojure CLI

**Build commands:**

```bash
bb build             # Build native binary
bb install           # Build + install to ~/.local/bin
bb install-system    # Install to /usr/local/bin (needs sudo)
```

## Run without installing

Just use Clojure directly:

```bash
git clone https://github.com/jeff-bruemmer/proserunner.git
cd proserunner
clojure -M:run --file /path/to/file.md
```

**You'll need:**

- Java
- Clojure CLI

## Pre-built binaries

Grab one from [releases](https://github.com/jeff-bruemmer/proserunner/releases), make it executable, toss it in your PATH.
