(ns benchmarks.core
  "Core benchmark utilities and data structures."
  (:require [clojure.java.io :as io]
            [clojure.string :as string]
            [proserunner.result :as result]
            [proserunner.vet.input :as input])
  (:gen-class))

(defrecord BenchmarkResult
  [name
   description
   iterations
   elapsed-ms
   mean-ms
   throughput
   throughput-unit])

(defn make-input
  "Builds the vet input for `options`. Throws on failure, so a broken setup
  can't be timed as no work."
  [options]
  (let [r (input/make options)]
    (if (result/failure? r)
      (throw (ex-info (str "Couldn't build benchmark input: " (:error r)) {:options options}))
      (:value r))))

(defn time-execution
  "Times the execution of f and returns elapsed time in milliseconds."
  [f]
  (let [start (System/nanoTime)
        result (f)
        end (System/nanoTime)]
    {:result result
     :elapsed-ms (/ (- end start) 1000000.0)}))

(defn run-benchmark
  "Runs a benchmark function multiple times and returns statistics.

   Options:
   - :iterations - number of times to run (default 5)
   - :warmup - number of warmup runs (default 2)
   - :throughput-fn - function to calculate throughput from result
   - :throughput-unit - unit for throughput (default 'items/sec')"
  [name description f {:keys [iterations warmup throughput-fn throughput-unit]
                       :or {iterations 5
                            warmup 2
                            throughput-unit "items/sec"}}]

  ;; Warmup runs
  (dotimes [_ warmup]
    (f))

  ;; Actual benchmark runs
  (let [runs (for [_ (range iterations)]
               (time-execution f))
        elapsed-times (map :elapsed-ms runs)
        total-elapsed (reduce + elapsed-times)
        mean-elapsed (/ total-elapsed iterations)

        ;; Calculate throughput if function provided
        throughput (when throughput-fn
                     (let [last-result (:result (last runs))]
                       (throughput-fn last-result mean-elapsed)))]

    (->BenchmarkResult
     name
     description
     iterations
     total-elapsed
     mean-elapsed
     throughput
     throughput-unit)))

(defn format-number
  "Format number with comma separators."
  [n]
  (if (nil? n)
    "N/A"
    (let [formatted (format "%.2f" (double n))]
      (string/replace formatted #"\B(?=(\d{3})+(?!\d))" ","))))

(defn print-result
  "Print a single benchmark result."
  [result]
  (println (format "\n%s" (:name result)))
  (println (format "  %s" (:description result)))
  (println (format "  Mean time: %s ms (over %d iterations)"
                   (format-number (:mean-ms result))
                   (:iterations result)))
  (when (:throughput result)
    (println (format "  Throughput: %s %s"
                     (format-number (:throughput result))
                     (:throughput-unit result)))))

(defn print-summary
  "Print summary of all benchmark results."
  [results]
  (println "\n" (string/join "" (repeat 80 "=")))
  (println " BENCHMARK SUMMARY")
  (println (string/join "" (repeat 80 "=")))
  (doseq [result results]
    (print-result result))
  (println "\n" (string/join "" (repeat 80 "="))))

(defn save-results
  "Save benchmark results to a file for tracking over time."
  [results filepath]
  (let [timestamp (.format (java.text.SimpleDateFormat. "yyyy-MM-dd HH:mm:ss")
                          (java.util.Date.))
        data {:timestamp timestamp
              :results (map #(into {} %) results)}]
    (spit filepath (pr-str data))
    (println (format "\nResults saved to: %s" filepath))))

(defn load-baseline
  "Load baseline results from a file."
  [filepath]
  (when (.exists (io/file filepath))
    (-> filepath
        slurp
        read-string)))

(defn compare-to-baseline
  "Compare current results to baseline and detect regressions.
   Returns a map with :regressions and :improvements."
  [current-results baseline-results threshold-pct]
  (let [baseline-map (into {} (map (fn [r] [(:name r) (:mean-ms r)])
                                  (:results baseline-results)))
        comparisons (for [current current-results
                         :let [baseline-time (get baseline-map (:name current))]]
                     (when baseline-time
                       (let [current-time (:mean-ms current)
                             pct-change (* 100 (/ (- current-time baseline-time)
                                                 baseline-time))
                             regression? (> pct-change threshold-pct)
                             improvement? (< pct-change (- threshold-pct))]
                         {:name (:name current)
                          :current-ms current-time
                          :baseline-ms baseline-time
                          :pct-change pct-change
                          :regression? regression?
                          :improvement? improvement?})))
        valid-comparisons (remove nil? comparisons)]
    {:comparisons valid-comparisons
     :regressions (filter :regression? valid-comparisons)
     :improvements (filter :improvement? valid-comparisons)}))

(defn print-comparison
  "Print comparison to baseline."
  [comparison threshold-pct]
  (println "\n" (string/join "" (repeat 80 "=")))
  (println " PERFORMANCE COMPARISON TO BASELINE")
  (println (format " Regression threshold: %.1f%%" threshold-pct))
  (println (string/join "" (repeat 80 "=")))

  (doseq [c (:comparisons comparison)]
    (let [symbol (cond
                   (:regression? c) "REGRESSION"
                   (:improvement? c) "IMPROVEMENT"
                   :else "STABLE")]
      (println (format "\n%s: %s" symbol (:name c)))
      (println (format "  Baseline: %s ms | Current: %s ms | Change: %+.1f%%"
                       (format-number (:baseline-ms c))
                       (format-number (:current-ms c))
                       (:pct-change c)))))

  (println "\n" (string/join "" (repeat 80 "=")))
  (println (format "Summary: %d regressions, %d improvements, %d stable"
                   (count (:regressions comparison))
                   (count (:improvements comparison))
                   (- (count (:comparisons comparison))
                      (count (:regressions comparison))
                      (count (:improvements comparison)))))
  (println (string/join "" (repeat 80 "="))))
