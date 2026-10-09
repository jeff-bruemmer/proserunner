(ns proserunner.result
  "Result type for error handling.

  A Result is either Success (contains value) or Failure (contains error and context).
  Enables error composition through bind without exceptions or System/exit.

  Key operations:
  - Constructors: ok, err
  - Predicates: success?, failure?
  - Composition: bind, fmap, traverse, sequence-results
  - Error handling: map-err, or-else, combine, combine-all-errors
  - Utilities: try-result, try-result-with-context, tap, result-or-exit"
  (:gen-class)
  (:require [proserunner.console :as console]))

(set! *warn-on-reflection* true)

;; Result type definitions
(defrecord Success
  [value])                    ; Any successful computation result

(defrecord Failure
  [^String error              ; Human-readable error message
   context])                  ; Map with additional context (e.g., {:file "x.md" :operation :read})

;; Constructor functions
(defn ok
  "Creates a Success result with the given value."
  [v]
  (->Success v))

(defn err
  "Creates a Failure result with an error message and optional context map."
  ([error-msg]
   (->Failure error-msg {}))
  ([error-msg context]
   (->Failure error-msg context)))

;; Type predicates
(defn success?
  "Returns true if result is a Success."
  [result]
  (instance? Success result))

(defn failure?
  "Returns true if result is a Failure."
  [result]
  (instance? Failure result))

;; Value extraction
(defn get-value
  "Extracts value from Success, returns nil for Failure."
  [result]
  (when (success? result)
    (:value result)))

(defn bind
  "Applies function f to Success value, short-circuits on Failure.

  f takes a value and returns a Result.
  Example: (bind (ok 5) (fn [x] (ok (* x 2)))) => (ok 10)"
  [result f]
  (if (success? result)
    (f (:value result))
    result))

(defn fmap
  "Maps function over a Success value, wrapping result in ok.

  f takes a value and returns a plain value.
  Example: (fmap (ok 5) (fn [x] (* x 2))) => (ok 10)"
  [result f]
  (if (success? result)
    (ok (f (:value result)))
    result))

;; Error recovery and composition
(defn or-else
  "Returns first successful result, or last failure if all fail.

  Example: (or-else (err \"fail 1\") (err \"fail 2\") (ok 3)) => (ok 3)"
  [& results]
  (or (first (filter success? results))
      (last results)))

(defn combine
  "Combines multiple results into a single result containing a vector of values.
  Returns Failure on first error."
  [results]
  (reduce (fn [acc result]
            (if (and (success? acc) (success? result))
              (ok (conj (:value acc) (:value result)))
              (or (when (failure? acc) acc)
                  result)))
          (ok [])
          results))

(defn combine-all-errors
  "Combines multiple results, collecting all errors.
  Returns Success with vector of values if all succeed,
  or Failure with all error messages if any fail."
  [results]
  (let [failures (filter failure? results)
        successes (filter success? results)]
    (if (empty? failures)
      (ok (mapv :value successes))
      (err "Multiple errors occurred"
           {:errors (mapv #(select-keys % [:error :context]) failures)
            :count (count failures)}))))

;; Try/catch wrappers
(defn try-result
  "Wraps a potentially throwing function in a Result.

  Returns Success with function result, or Failure with exception message.
  Optional error-fn transforms exception to custom error message."
  ([f]
   (try-result f identity))
  ([f error-fn]
   (try
     (ok (f))
     (catch Exception e
       (err (error-fn e) {:exception e :message (.getMessage e)})))))

(defn- result?
  [v]
  (or (instance? Success v) (instance? Failure v)))

(defn try-result-with-context
  "Like try-result but with custom context for errors.
  If f itself returns a Result, it is returned as is rather than
  wrapped in another Success, so failures aren't masked."
  [f context-map]
  (try
    (let [v (f)]
      (if (result? v) v (ok v)))
    (catch Exception e
      (err (.getMessage e)
           (merge context-map
                  {:exception e
                   :exception-type (class e)})))))

;; Output helpers
(defn print-failure
  "Prints a failure's message to stderr without exiting.
  The context map (which can hold exceptions) prints only when PROSERUNNER_DEBUG is set."
  [result]
  (when (failure? result)
    (console/error (:error result))
    (when (and (console/debug?) (seq (:context result)))
      (console/warn "Details: " (pr-str (:context result))))))

;; Advanced combinators

(defn traverse
  "Applies a Result-returning function to each element in a collection.
  Returns Success with vector of values if all succeed, or first Failure.

  f takes a value and returns a Result.
  Example: (traverse parse-int [\"1\" \"2\" \"3\"]) => (ok [1 2 3])"
  [f coll]
  (reduce (fn [acc item]
            (if (success? acc)
              (let [result (f item)]
                (if (success? result)
                  (ok (conj (:value acc) (:value result)))
                  result))
              acc))
          (ok [])
          coll))

(defn sequence-results
  "Converts a collection of Results into a Result containing a vector.
  Returns Success with all values if all succeed, or first Failure.

  Example: (sequence-results [(ok 1) (ok 2) (ok 3)]) => (ok [1 2 3])"
  [results]
  (traverse identity results))

(defn map-err
  "Transforms the error message in a Failure, preserving context.
  Does nothing to Success results.

  f takes an error value and returns a new error value.
  Example: (map-err (err \"failed\") #(str \"ERROR: \" %)) => (err \"ERROR: failed\")"
  [result f]
  (if (failure? result)
    (->Failure (f (:error result)) (:context result))
    result))

(defn tap
  "Executes a side-effecting function on Success value, returns original result.
  Does nothing on Failure. Useful for logging/debugging in a chain.

  f takes a value and returns anything (return value is ignored).
  Example: (tap (ok 42) println) => (ok 42) ; prints 42"
  [result f]
  (when (success? result)
    (f (:value result)))
  result)

;; Dynamic var for exit function (allows testing)
(def ^:dynamic *exit-fn* (fn [code] (System/exit code)))

(defn result-or-exit
  "Extracts value from Success, or prints error and exits on Failure.
  This is the boundary function for converting Results to plain values at app entry points.

  Optional exit-code parameter (default 2, the error exit code).
  Example: (result-or-exit (ok 42)) => 42
           (result-or-exit (err \"failed\")) => exits with code 2"
  ([result]
   (result-or-exit result 2))
  ([result exit-code]
   (if (success? result)
     (:value result)
     (do
       (print-failure result)
       (*exit-fn* exit-code)))))
