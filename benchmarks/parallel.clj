(ns benchmarks.parallel
  "Parallel benchmark runner - tests performance with parallel file processing."
  (:require [benchmarks.core :as bench]
            [proserunner.system :as sys]
            [proserunner.vet :as vet]
            [clojure.java.io :as io]
            [clojure.string :as string]
            [editors.registry :as registry]
            [editors.utilities :as util]
            [editors.repetition :as repetition]
            [editors.re :as re])
  (:gen-class))

(defn setup-editors!
  "Register all editors."
  []
  (doseq [editor-type (util/standard-editor-types)]
    (registry/register-editor! editor-type (util/create-editor editor-type)))
  (registry/register-editor! "repetition" repetition/proofread)
  (registry/register-editor! "regex" re/proofread))

(defn- make-benchmark-input
  "Create a vet input map with standard benchmark settings.
   Disables caching and parallel processing to isolate file-level parallelism."
  [file config-path]
  (bench/make-input {:file file
                     :config config-path
                     :output "table"
                     :code-blocks false
                     :no-cache true
                     :sequential-lines true}))

(defn- process-file
  "Process a single file with vet/compute, realizing its lazy results."
  [file config-path]
  (doall (:results (vet/compute (make-benchmark-input file config-path)))))

(defn- count-lines
  "Count total lines across multiple files."
  [files]
  (reduce + (map (fn [f]
                   (with-open [rdr (io/reader f)]
                     (count (line-seq rdr))))
                 files)))

(defn benchmark-parallel-processing
  "Benchmark processing multiple files in parallel."
  [config-path]
  (let [files ["resources/benchmark-data/small.md"
               "resources/benchmark-data/medium.md"]
        existing-files (filter #(.exists (io/file %)) files)
        total-lines (count-lines existing-files)]

    (when (seq existing-files)
      (bench/run-benchmark
       "Parallel File Processing"
       (format "Processing %d files in parallel (%d total lines)"
               (count existing-files) total-lines)
       #(doall (pmap (fn [file] (process-file file config-path))
                     existing-files))
       {:iterations 10
        :warmup 3
        :throughput-fn (fn [_ mean-ms] (/ total-lines (/ mean-ms 1000)))
        :throughput-unit "lines/sec"}))))

(defn benchmark-sequential-vs-parallel
  "Compare sequential vs parallel processing performance."
  [config-path]
  (let [files ["resources/benchmark-data/small.md"
               "resources/benchmark-data/medium.md"]
        existing-files (filter #(.exists (io/file %)) files)
        total-lines (count-lines existing-files)
        benchmark-opts {:iterations 10
                        :warmup 3
                        :throughput-fn (fn [_ mean-ms] (/ total-lines (/ mean-ms 1000)))
                        :throughput-unit "lines/sec"}]

    (when (seq existing-files)
      ;; Sequential benchmark
      (let [seq-result (bench/run-benchmark
                        "Sequential Processing"
                        (format "%d files sequentially" (count existing-files))
                        #(doall (map (fn [file] (process-file file config-path))
                                     existing-files))
                        benchmark-opts)

            ;; Parallel benchmark
            par-result (bench/run-benchmark
                        "Parallel Processing"
                        (format "%d files in parallel" (count existing-files))
                        #(doall (pmap (fn [file] (process-file file config-path))
                                      existing-files))
                        benchmark-opts)]

        [seq-result par-result]))))

(defn -main
  "Main entry point for parallel benchmark runner."
  [& _args]
  (println "\n╔════════════════════════════════════════════════════════════════════════════╗")
  (println "║              PROSERUNNER PARALLEL PROCESSING BENCHMARKS                   ║")
  (println "╚════════════════════════════════════════════════════════════════════════════╝\n")

  (setup-editors!)

  (let [config-path (sys/config-path "config.edn")]

    ;; Check for benchmark files
    (when-not (.exists (io/file "resources/benchmark-data/small.md"))
      (println "Error: resources/benchmark-data/ files not found.")
      (println "These files are required for parallel benchmarks.\n")
      (System/exit 1))

    (println "Testing parallel processing performance...\n")

    ;; Run benchmarks
    (let [results (concat
                   [(benchmark-parallel-processing config-path)]
                   (benchmark-sequential-vs-parallel config-path))
          valid-results (remove nil? results)]

      (bench/print-summary valid-results)

      ;; Calculate speedup
      (when (= (count valid-results) 3)
        (let [seq-time (:mean-ms (first (filter #(= (:name %) "Sequential Processing") valid-results)))
              par-time (:mean-ms (first (filter #(= (:name %) "Parallel Processing") valid-results)))
              speedup (/ seq-time par-time)]
          (println "\n" (string/join "" (repeat 80 "=")))
          (println " PARALLEL SPEEDUP ANALYSIS")
          (println (string/join "" (repeat 80 "=")))
          (println (format "\nSequential: %.2f ms" seq-time))
          (println (format "Parallel:   %.2f ms" par-time))
          (println (format "Speedup:    %.2fx" speedup))
          (println "\n" (string/join "" (repeat 80 "="))))))
    (shutdown-agents)))
