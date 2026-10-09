(ns proserunner.file-utils
  "File utility functions for safe file operations."
  (:require [clojure.java.io :as io]
            [clojure.string :as string]
            [proserunner.console :as console]
            [proserunner.result :as result])
  (:gen-class))

(set! *warn-on-reflection* true)

(defn absolute-path?
  "Check if a path is absolute."
  [path]
  (or (.isAbsolute (io/file path))
      (string/starts-with? path "~")))

(defn join-path
  "Joins path components using the system's file separator.

  Examples:
    (join-path \"home\" \"user\" \"file.txt\") => \"home/user/file.txt\" (on Unix)
    (join-path \"C:\" \"Users\" \"file.txt\") => \"C:\\Users\\file.txt\" (on Windows)"
  [& components]
  (string/join java.io.File/separator components))

(defn atomic-spit
  "Atomically writes content to a file by writing to a temp file and renaming.
   This prevents partial writes and race conditions during concurrent updates.

   On failure, attempts to clean up the temporary file. If cleanup fails,
   logs a warning but still propagates the original exception."
  [file-path content]
  (let [file (io/file file-path)
        parent-dir (.getParentFile file)
        temp-file (java.io.File/createTempFile ".proserunner-" ".tmp" parent-dir)]
    (try
      ;; Write to temp file
      (spit temp-file content)
      ;; Atomic rename (overwrites target if exists)
      (java.nio.file.Files/move
       (.toPath temp-file)
       (.toPath file)
       (into-array java.nio.file.StandardCopyOption
                   [java.nio.file.StandardCopyOption/ATOMIC_MOVE
                    java.nio.file.StandardCopyOption/REPLACE_EXISTING]))
      (catch Exception e
        ;; Clean up temp file on failure with nested try/catch
        (try
          (.delete temp-file)
          (catch Exception cleanup-error
            ;; Log cleanup failure but don't mask original error
            (binding [*out* *err*]
              (println (str "Warning: Failed to cleanup temp file " (.getName temp-file)
                           ": " (.getMessage cleanup-error))))))
        ;; Always rethrow original exception
        (throw e)))))

(def ^:private lock-monitor
  "Serializes locking within this process: a second FileChannel lock on the
  same file from one JVM throws instead of waiting."
  (Object.))

(def ^:private ^:dynamic *locked?*
  "True while this thread holds the file lock, so nested calls don't relock."
  false)

(defn- open-lock-channel
  "Opens `lock-path` for locking, creating it, or returns nil if it can't."
  ^java.nio.channels.FileChannel [lock-path]
  (try
    (.mkdirs (.getParentFile (.getAbsoluteFile (io/file lock-path))))
    (java.nio.channels.FileChannel/open
     (.toPath (io/file lock-path))
     (into-array java.nio.file.OpenOption
                 [java.nio.file.StandardOpenOption/CREATE
                  java.nio.file.StandardOpenOption/WRITE]))
    (catch java.io.IOException _ nil)))

(defn call-with-lock
  "Calls f while holding an exclusive lock on the file at `lock-path`, so
  read-modify-write updates from concurrent runs don't lose each other's
  changes. Waits for another process holding it, saying so. Reentrant.

  atomic-spit already keeps each write whole; this keeps the read and the
  write together. If the lock file can't be created (say, a read-only home),
  f runs unlocked rather than failing. Closing the channel releases the
  lock, and the OS releases it if the process dies."
  [lock-path f]
  (if *locked?*
    (f)
    (locking lock-monitor
      (if-let [ch (open-lock-channel lock-path)]
        (with-open [ch ch]
          (when-not (.tryLock ch)
            (console/status "Waiting for another proserunner to finish...")
            (.lock ch))
          (binding [*locked?* true]
            (f)))
        (f)))))

(defn ensure-parent-dir
  "Ensures that the parent directory of the given filepath exists.
  Creates all necessary parent directories if they don't exist.

  Example:
    (ensure-parent-dir \"/path/to/nested/file.txt\")
    ;; Creates /path/to/nested/ if it doesn't exist"
  [filepath]
  (when-let [parent-file (.getParentFile (io/file filepath))]
    (.mkdirs parent-file)))

(defn write-edn-file
  "Writes EDN data to a file, creating parent directories if needed.
  Returns Result<filepath> on success, Failure on error.

  Uses atomic-spit for safe concurrent writes.

  Example:
    (write-edn-file \"config.edn\" {:foo \"bar\"})
    ;; => #proserunner.result.Success{:value \"config.edn\"}"
  [filepath data]
  (result/try-result-with-context
   (fn []
     (ensure-parent-dir filepath)
     (atomic-spit filepath (pr-str data))
     filepath)
   {:filepath filepath :operation :write-edn-file}))

(defn mkdirs-if-missing
  "Creates a directory and all necessary parent directories if they don't exist.
  Idempotent - safe to call multiple times.

  Example:
    (mkdirs-if-missing \"/path/to/nested/dir\")
    ;; Creates the entire directory structure"
  [dirpath]
  (.mkdirs (io/file dirpath)))

(defn delete-tree!
  "Deletes a file, or a directory and everything in it. Doesn't follow
  symlinks, so it never deletes outside `path`. Missing paths are fine."
  [path]
  (let [f (io/file path)]
    (when (and (.isDirectory f)
               (not (java.nio.file.Files/isSymbolicLink (.toPath f))))
      (doseq [child (.listFiles f)]
        (delete-tree! child)))
    (.delete f)))

(defn normalize-path
  "Normalizes a file path to be relative to the current working directory.
  This ensures consistent path representation across the application.

  Examples:
    ;; When cwd is /home/user/project
    (normalize-path \"/home/user/project/docs/file.md\")
    ;; => \"docs/file.md\"

    (normalize-path \"docs/file.md\")
    ;; => \"docs/file.md\"

    (normalize-path \"./docs/file.md\")
    ;; => \"docs/file.md\""
  [filepath]
  (let [file (io/file filepath)
        absolute-file (.getAbsoluteFile file)
        cwd (io/file (System/getProperty "user.dir"))
        absolute-cwd (.getAbsoluteFile cwd)]
    (if (.startsWith (.toPath absolute-file) (.toPath absolute-cwd))
      ;; File is under working directory, make it relative
      (str (.relativize (.toPath absolute-cwd) (.toPath absolute-file)))
      ;; File is outside working directory, return absolute path
      (.getPath absolute-file))))
