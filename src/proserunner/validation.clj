(ns proserunner.validation
  "Composable validation combinators for building reusable validators.
   Provides a functional approach to validation with clear error messaging."
  (:require [clojure.string :as string])
  (:gen-class))

(set! *warn-on-reflection* true)

;;; Core Validation Results

(defn validation-ok
  "Creates a successful validation result."
  [value]
  {:valid? true :value value :errors []})

(defn validation-error
  "Creates a failed validation result."
  [value & error-msgs]
  {:valid? false :value value :errors (vec error-msgs)})

;;; Primitive Validators
;;; Each returns a function: value -> validation-result

(defn required
  "Validator that ensures value is not nil."
  [field-name]
  (fn [value]
    (if (nil? value)
      (validation-error value (str field-name " is required"))
      (validation-ok value))))

(defn type-check
  "Validator that ensures value is of specified type using predicate."
  [field-name pred type-desc]
  (fn [value]
    (if (and value (not (pred value)))
      (validation-error value
        (str field-name " must be " type-desc ", got " (type value)))
      (validation-ok value))))

(defn not-empty-check
  "Validator that ensures collection is not empty."
  [field-name]
  (fn [value]
    (if (and value (empty? value))
      (validation-error value (str field-name " cannot be empty"))
      (validation-ok value))))

(defn enum-check
  "Validator that ensures value is one of the allowed values."
  [field-name allowed-values]
  (fn [value]
    (if (and value (not (contains? allowed-values value)))
      (validation-error value
        (str field-name " must be one of " (pr-str allowed-values) ", got " (pr-str value)))
      (validation-ok value))))

;;; Combinators
;;; Compose multiple validators into one

(defn chain
  "Chains validators together, stopping at first error.
   Short-circuits on first validation failure."
  [& validators]
  (fn [value]
    (reduce
      (fn [result validator]
        (if (:valid? result)
          (validator (:value result))
          (reduced result)))
      (validation-ok value)
      validators)))

(defn with-default
  "Wraps a validator chain, applying default value if input is nil."
  [default-val validator]
  (fn [value]
    (if (nil? value)
      (validation-ok default-val)
      (validator value))))

;;; Field validation aggregation

(defn validate-fields
  "Validates multiple fields in a map, collecting all errors.

   validators-map: {field-keyword validator-fn}
   data: map to validate

   Returns {:valid? bool :data map :errors [strings]}"
  [validators-map data]
  (let [validations (into {}
                      (map (fn [[field validator-fn]]
                             [field (validator-fn (get data field))])
                           validators-map))
        all-errors (mapcat :errors (vals validations))
        valid? (empty? all-errors)
        validated-data (into {} (map (fn [[k v]] [k (:value v)]) validations))]
    {:valid? valid?
     :data validated-data
     :errors all-errors}))

(defn validate!
  "Validates data using validators-map, throwing on error.
   Returns validated data with defaults applied on success."
  [validators-map data]
  (let [result (validate-fields validators-map data)]
    (if (:valid? result)
      (:data result)
      (let [error-msg (if (= 1 (count (:errors result)))
                        (first (:errors result))
                        (str "Validation errors:\n"
                             (string/join "\n"
                               (map #(str "  - " %) (:errors result)))))]
        (throw (ex-info error-msg
                        {:errors (:errors result)
                         :data data}))))))
