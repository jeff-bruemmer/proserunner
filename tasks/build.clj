(ns tasks.build
  (:require [babashka.fs :as fs]
            [babashka.process :refer [shell]]
            [clojure.string :as str]
            [tasks.util :refer [run-shell run-shell-check print-lines get-file-size]]))

;; Pure functions

(defn parse-platform-info
  "Parse platform information from uname output."
  [os arch]
  (let [os-trimmed (str/trim os)
        arch-trimmed (str/trim arch)]
    {:os os-trimmed
     :arch arch-trimmed}))

(defn format-version-info
  "Format version information map."
  [java-version clojure-version]
  [(str "  Java: " java-version)
   (str "  Clojure: " clojure-version)])

(def build-artifacts
  "List of build artifacts to clean."
  ["classes" ".cpcache"])

(def binary-name
  "Binary name for native image build."
  "proserunner")

(def test-content
  "Test markdown content for benchmarking."
  "# Test Document

This is a test document with various content that proserunner will check.

## Code Example

```bash
echo \"Hello world\"
```

## Regular Text

The quick brown fox jumps over the lazy dog.

> Quoted text

More regular prose.")

;; Prerequisite checks

(defn check-command!
  "Check if command exists, exit with error if not."
  [cmd error-msgs]
  (when-not (fs/which cmd)
    (println (format "\nERROR: %s command not found." cmd))
    (print-lines error-msgs)
    (System/exit 1)))

(defn check-prerequisites!
  "Check all prerequisites for build."
  []
  (println "Checking prerequisites...")

  (check-command! "clojure"
                  ["Please install Clojure CLI tools: https://clojure.org/guides/install_clojure"])

  (check-command! "native-image"
                  ["\nGraalVM native-image is required to build the native binary."
                   "\nInstallation options:"
                   "  1. Download GraalVM: https://www.graalvm.org/downloads/"
                   "  2. Or use SDKMAN: sdk install java 21-graal\n"
                   "After installing GraalVM, make sure native-image is in your PATH:"
                   "  export JAVA_HOME=/path/to/graalvm"
                   "  export PATH=$JAVA_HOME/bin:$PATH"])

  (let [java-ver (run-shell "java -version 2>&1 | head -1")
        clojure-ver (run-shell "clojure --version")]
    (println "✓ Prerequisites found")
    (print-lines (format-version-info java-ver clojure-ver))))

(defn detect-platform!
  "Detect and print platform information, return config."
  []
  (let [os (run-shell "uname -s")
        arch (run-shell "uname -m")
        platform (parse-platform-info os arch)]
    (println "\nDetected platform:" (:os platform) (:arch platform))
    platform))

;; File operations (side effects)

(defn safe-to-delete?
  "Validate that path is safe to delete (relative and within current directory)."
  [path]
  (let [path-str (str path)]
    (and
     (not (str/starts-with? path-str "/"))
     (not (str/includes? path-str ".."))
     (not (str/blank? path-str)))))

(defn cleanup-artifacts!
  "Clean up build artifacts safely (only relative paths in current directory)."
  [artifacts]
  (doseq [artifact artifacts]
    (when-not (safe-to-delete? artifact)
      (println (str "WARNING: Skipping unsafe deletion path: " artifact))
      (throw (ex-info "Attempted to delete unsafe path" {:path artifact})))
    (when (fs/exists? artifact)
      (if (fs/directory? artifact)
        (fs/delete-tree artifact)
        (fs/delete artifact)))))

(defn verify-binary!
  "Verify binary exists, exit if not."
  [binary-path]
  (when-not (fs/exists? binary-path)
    (println "\nERROR: Build failed - proserunner binary not found")
    (System/exit 1)))

(defn make-executable!
  "Make file executable."
  [file-path]
  (fs/set-posix-file-permissions file-path "rwxr-xr-x"))

;; Build workflow

(defn compile-clojure!
  "AOT compile Clojure code to classes."
  []
  (println "Compiling Clojure code...")
  (fs/create-dirs "classes")
  (let [result (shell {:continue true} "clojure -e \"(require 'proserunner.core) (compile 'proserunner.core)\"")]
    (when-not (zero? (:exit result))
      (println "\nERROR: Compilation failed")
      (System/exit (:exit result)))))

(defn get-classpath!
  "Get the classpath for native-image build."
  []
  (println "Generating classpath...")
  (let [result (shell {:out :string} "clojure -Spath")]
    (str/trim (:out result))))

(defn native-image-options
  "Generate native-image options based on platform."
  [platform]
  (let [common-opts ["-H:+UnlockExperimentalVMOptions"
                     "-H:Name=proserunner"
                     "-Dclojure.compiler.direct-linking=true"
                     "--initialize-at-build-time=clojure,proserunner,editors,babashka,cheshire,com.fasterxml.jackson"
                     "-H:EnableURLProtocols=http,https"
                     "--enable-http"
                     "--enable-https"
                     "--no-fallback"
                     "-O2"
                     "--gc=serial"]
        arch-opt (if (and (= (:os platform) "Darwin")
                         (= (:arch platform) "arm64"))
                  "-march=armv8-a"
                  "-march=x86-64-v2")]
    (conj common-opts arch-opt)))

(defn run-native-image!
  "Execute native-image build."
  [platform classpath]
  (println "\nBuilding native image...")
  (println "This could take 30-60 seconds...\n")
  (let [opts (native-image-options platform)
        ;; Pass argv as a vector so classpath entries containing spaces aren't split
        cmd (into ["native-image" "-cp" classpath] (conj opts "proserunner.core"))
        result (apply shell {:continue true :inherit true} cmd)]
    (when-not (zero? (:exit result))
      (println "\nERROR: Native image build failed")
      (System/exit (:exit result)))))

(defn test-binary!
  "Test binary by running help command."
  [binary-path]
  (println "\nTesting binary...")
  (shell (str binary-path " -h")))

(defn show-build-info
  "Display build information."
  [binary-path]
  (print-lines ["\nBinary location:" (str (fs/cwd) "/" binary-path)
                "Binary size:" (get-file-size binary-path)]))

(defn build
  "Build native binary with GraalVM."
  []
  (println "=== Proserunner Build ===\n")

  (check-prerequisites!)
  (let [platform (detect-platform!)]

    (println "\nCleaning build artifacts...")
    (cleanup-artifacts! build-artifacts)

    (compile-clojure!)
    ;; deps.edn :paths already includes "classes"
    (run-native-image! platform (get-classpath!))

    (verify-binary! binary-name)
    (make-executable! binary-name)

    (println "\nCleaning up build artifacts...")
    (cleanup-artifacts! build-artifacts)

    (println "\n✓ Build successful!\n")
    (test-binary! (str "./" binary-name))
    (show-build-info binary-name)))

;; Benchmark

(defn run-benchmark-iterations
  "Run benchmark iterations and return times."
  [binary test-file iterations]
  (mapv (fn [i]
          (let [start (System/nanoTime)
                _ (run-shell-check (str binary " -f " test-file " > /dev/null 2>&1"))
                elapsed (/ (- (System/nanoTime) start) 1e9)]
            (println (format "Run %d: %.3fs" (inc i) elapsed))
            elapsed))
        (range iterations)))

(defn calculate-average
  "Calculate average of numeric collection."
  [coll]
  (/ (reduce + coll) (count coll)))

(defn benchmark-build
  "Benchmark the build performance."
  []
  (println "=== Build Performance Comparison ===\n")

  (when-not (fs/exists? binary-name)
    (println (str "ERROR: " binary-name " not found. Run bb build first."))
    (System/exit 1))

  (let [test-file (str (fs/create-temp-file {:prefix "benchmark-test" :suffix ".md"}))
        binary (str "./" binary-name)
        iterations 10]

    (spit test-file test-content)
    (print-lines ["Running benchmark with current binary..."
                  (str "Test file: " test-file "\n")])

    ;; Warm-up
    (run-shell-check (str binary " -f " test-file " > /dev/null 2>&1"))

    ;; Benchmark
    (println "Timing" iterations "runs...")
    (let [times (run-benchmark-iterations binary test-file iterations)
          avg (calculate-average times)]

      (println (format "\nAverage time: %.3fs\n" avg))
      (fs/delete test-file)

      (print-lines ["Binary info:"
                    (str "  Size: " (get-file-size binary-name))
                    (str "  Build: " (run-shell (str "./" binary-name " --version")))]))))
