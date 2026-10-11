(ns proserunner.path-ignore
  "Functions for handling path-based ignores (like .gitignore)."
  (:gen-class)
  (:require [clojure.java.io :as io]
            [clojure.string :as string]
            [proserunner.result :as result]))

(set! *warn-on-reflection* true)

(defn read-proserunnerignore
  "Reads .proserunnerignore file from the base directory and returns Result of patterns.

  Returns Success with vector of patterns, or Failure if file can't be read.
  If file doesn't exist, returns Success with empty vector."
  [base-dir]
  (let [ignore-file (io/file base-dir ".proserunnerignore")
        filepath (.getAbsolutePath ignore-file)]
    (if (.exists ignore-file)
      (result/try-result-with-context
        (fn []
          (->> (slurp ignore-file)
               (string/split-lines)
               (map string/trim)
               (remove #(or (string/blank? %) (string/starts-with? % "#")))
               (vec)))
        {:operation :read-proserunnerignore
         :filepath filepath})
      (result/ok []))))

(defn glob-to-regex
  "Converts a glob pattern to a regex pattern.
   Supports:
   - `*` matches any characters except `/`
   - `**` matches zero or more path segments (including `/`)
   - `?` matches a single character (except `/`)
   - All regex metacharacters are properly escaped

   The resulting pattern is anchored at the start and matches from beginning of path."
  [pattern]
  (let [escaped (-> pattern
                    ;; First, escape all regex metacharacters (except * and ? which we handle specially)
                    (string/replace #"([\\^$.|+(){}\[\]])" "\\\\$1")
                    (string/replace "**" "§§DOUBLESTAR§§")  ; Temporarily protect **
                    (string/replace "*" "[^/]*")            ; * matches any char except /
                    (string/replace "?" "[^/]")             ; ? matches single char except /
                    ;; ** can match zero or more path segments
                    ;; **/ at start means optional leading path
                    (string/replace #"§§DOUBLESTAR§§/" "(.*/)?")
                    ;; /** means slash followed by anything (slash is required)
                    (string/replace #"/§§DOUBLESTAR§§" "/.*")
                    ;; Standalone ** matches anything
                    (string/replace "§§DOUBLESTAR§§" ".*"))]
    ;; Anchor at the start of the string
    (re-pattern (str "^" escaped))))

(defn should-ignore?
  "Checks if a file path should be ignored based on patterns.

  Compares the path relative to base-dir against each glob pattern. Both are
  made absolute first, since file-seq on a relative directory yields
  relative paths like ./drafts/x.md while base-dir is absolute.
  Returns truthy if any pattern matches, nil otherwise."
  [file-path base-dir patterns]
  (let [absolute #(.normalize (.toPath (.getAbsoluteFile (io/file %))))
        ^java.nio.file.Path base (absolute base-dir)
        ^java.nio.file.Path path (absolute file-path)
        relative-path (if (.startsWith path base)
                        (str (.relativize base path))
                        file-path)]
    (some (fn [pattern]
            (let [regex (glob-to-regex pattern)]
              ;; Use re-find to allow patterns to match anywhere in the path
              ;; For example, "*.md" matches "file.md" in "dir/file.md"
              ;; But "*.md" anchored would not match "dir/file.md"
              (re-find regex relative-path)))
          patterns)))
