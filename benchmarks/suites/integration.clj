(ns benchmarks.suites.integration
  "End-to-end integration benchmarks using real files."
  (:require [benchmarks.core :as bench]
            [proserunner.result :as result]
            [proserunner.system :as sys]
            [proserunner.vet :as vet]
            [clojure.java.io :as io]
            [editors.registry :as registry]
            [editors.utilities :as util]
            [editors.repetition :as repetition]
            [editors.re :as re]))

(defn setup-editors!
  "Register all editors for benchmarking."
  []
  (doseq [editor-type (util/standard-editor-types)]
    (registry/register-editor! editor-type (util/create-editor editor-type)))
  (registry/register-editor! "repetition" repetition/proofread)
  (registry/register-editor! "regex" re/proofread))

(defn count-lines-in-file
  "Count lines in a file."
  [filepath]
  (with-open [rdr (io/reader filepath)]
    (count (line-seq rdr))))

(defn benchmark-file-processing
  "Benchmark processing a specific file."
  [file-path config-path description]
  (when (.exists (io/file file-path))
    (let [line-count (count-lines-in-file file-path)
          input (bench/make-input {:file file-path
                                   :config config-path
                                   :output "table"
                                   :code-blocks false
                                   :no-cache true})]

      (bench/run-benchmark
       (str "File: " (.getName (io/file file-path)))
       (format "%s (%d lines)" description line-count)
       ;; :results is lazy; realize it so the checks run inside the timing
       #(doall (:results (vet/compute input)))
       {:iterations 5
        :warmup 2
        :throughput-fn (fn [_ mean-ms] (/ line-count (/ mean-ms 1000)))
        :throughput-unit "lines/sec"}))))

(defn benchmark-small-file
  "Benchmark processing a small file (~10 lines)."
  [config-path]
  (benchmark-file-processing
   "resources/benchmark-data/small.md"
   config-path
   "Small file"))

(defn benchmark-medium-file
  "Benchmark processing a medium file (~100 lines)."
  [config-path]
  (benchmark-file-processing
   "resources/benchmark-data/medium.md"
   config-path
   "Medium file"))

(defn benchmark-large-file
  "Benchmark processing a large file (~1000 lines)."
  [config-path]
  (benchmark-file-processing
   "resources/benchmark-data/large.md"
   config-path
   "Large file"))

(defn benchmark-xlarge-file
  "Benchmark processing an extra large file (~5000 lines)."
  [config-path]
  (benchmark-file-processing
   "resources/benchmark-data/xlarge.md"
   config-path
   "Extra large file"))

(defn benchmark-cache-performance
  "Benchmark a cache hit: rerunning an unchanged file, from reading it to
  returning the cached results. Uses a throwaway cache directory."
  [config-path]
  (when (.exists (io/file "resources/benchmark-data/large.md"))
    (let [file-path "resources/benchmark-data/large.md"
          cache-dir (str (java.nio.file.Files/createTempDirectory
                          "proserunner-bench-cache"
                          (make-array java.nio.file.attribute.FileAttribute 0)))
          run #(let [r (vet/compute-or-cached {:file file-path
                                               :config config-path
                                               :output "table"
                                               :code-blocks false
                                               :cache-dir cache-dir})]
                 (when (result/failure? r)
                   (throw (ex-info (str "Cached run failed: " (:error r)) {})))
                 r)]
      (try
        ;; First run to populate cache
        (run)

        (bench/run-benchmark
         "Cache Performance"
         "Cached file processing (should be very fast)"
         run
         {:iterations 10
          :warmup 2
          :throughput-fn (fn [_ mean-ms] (/ (count-lines-in-file file-path) (/ mean-ms 1000)))
          :throughput-unit "lines/sec"})
        (finally
          (run! io/delete-file (reverse (file-seq (io/file cache-dir)))))))))

(defn run-all-integration-benchmarks
  "Run all integration benchmarks and return results."
  []
  (setup-editors!)
  (let [config-path (sys/config-path "config.edn")]
    (remove nil?
            [(benchmark-small-file config-path)
             (benchmark-medium-file config-path)
             (benchmark-large-file config-path)
             (benchmark-xlarge-file config-path)
             (benchmark-cache-performance config-path)])))
