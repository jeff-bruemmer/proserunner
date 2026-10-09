(ns proserunner.result-test
  "Tests for the Result type."
  (:require [clojure.string]
            [clojure.test :refer [deftest is testing]]
            [proserunner.result :as result]))

(deftest success-creation-test
  (testing "Creating success results"
    (let [r (result/ok 42)]
      (is (result/success? r))
      (is (not (result/failure? r)))
      (is (= 42 (:value r))))))

(deftest failure-creation-test
  (testing "Creating failure results"
    (let [r (result/err "Something went wrong" {:code 404})]
      (is (result/failure? r))
      (is (not (result/success? r)))
      (is (= "Something went wrong" (:error r)))
      (is (= {:code 404} (:context r))))))

(deftest bind-test
  (testing "Bind with successful results"
    (let [r1 (result/ok 5)
          r2 (result/bind r1 (fn [x] (result/ok (* x 2))))]
      (is (result/success? r2))
      (is (= 10 (:value r2)))))

  (testing "Bind short-circuits on failure"
    (let [r1 (result/err "Failed" {})
          r2 (result/bind r1 (fn [x] (result/ok (* x 2))))]
      (is (result/failure? r2))
      (is (= "Failed" (:error r2)))))

  (testing "Bind propagates failure through chain"
    (let [r (-> (result/ok 5)
                (result/bind (fn [x] (result/ok (* x 2))))
                (result/bind (fn [_] (result/err "Chain broken" {})))
                (result/bind (fn [x] (result/ok (* x 3)))))]
      (is (result/failure? r))
      (is (= "Chain broken" (:error r))))))

(deftest fmap-test
  (testing "fmap applies function to success value"
    (let [r1 (result/ok 5)
          r2 (result/fmap r1 #(* % 2))]
      (is (result/success? r2))
      (is (= 10 (:value r2)))))

  (testing "fmap short-circuits on failure"
    (let [r1 (result/err "Failed" {})
          r2 (result/fmap r1 #(* % 2))]
      (is (result/failure? r2))
      (is (= "Failed" (:error r2))))))

(deftest or-else-test
  (testing "or-else returns first success"
    (let [r1 (result/err "Fail 1" {})
          r2 (result/err "Fail 2" {})
          r3 (result/ok 42)
          r4 (result/ok 100)
          result (result/or-else r1 r2 r3 r4)]
      (is (result/success? result))
      (is (= 42 (:value result)))))

  (testing "or-else returns last failure when all fail"
    (let [r1 (result/err "Fail 1" {})
          r2 (result/err "Fail 2" {})
          r3 (result/err "Fail 3" {})
          result (result/or-else r1 r2 r3)]
      (is (result/failure? result))
      (is (= "Fail 3" (:error result))))))

(deftest combine-test
  (testing "combine succeeds when all succeed"
    (let [results [(result/ok 1) (result/ok 2) (result/ok 3)]
          combined (result/combine results)]
      (is (result/success? combined))
      (is (= [1 2 3] (:value combined)))))

  (testing "combine fails on first failure"
    (let [results [(result/ok 1)
                   (result/err "Failed" {})
                   (result/ok 3)]
          combined (result/combine results)]
      (is (result/failure? combined))
      (is (= "Failed" (:error combined))))))

(deftest combine-all-errors-test
  (testing "combine-all-errors collects all failures"
    (let [results [(result/ok 1)
                   (result/err "Fail 1" {})
                   (result/ok 3)
                   (result/err "Fail 2" {})]
          combined (result/combine-all-errors results)]
      (is (result/failure? combined))
      (is (= "Multiple errors occurred" (:error combined)))
      (is (= 2 (-> combined :context :count)))))

  (testing "combine-all-errors succeeds when all succeed"
    (let [results [(result/ok 1) (result/ok 2) (result/ok 3)]
          combined (result/combine-all-errors results)]
      (is (result/success? combined))
      (is (= [1 2 3] (:value combined))))))

(deftest try-result-test
  (testing "try-result wraps successful function"
    (let [r (result/try-result #(+ 1 2))]
      (is (result/success? r))
      (is (= 3 (:value r)))))

  (testing "try-result catches exceptions"
    (let [r (result/try-result #(throw (Exception. "Boom!")))]
      (is (result/failure? r))
      (is (instance? Exception (:error r)))))

  (testing "try-result with custom error function"
    (let [r (result/try-result
             #(throw (Exception. "Boom!"))
             (fn [e] (str "Custom: " (.getMessage e))))]
      (is (result/failure? r))
      (is (= "Custom: Boom!" (:error r))))))

(deftest try-result-with-context-test
  (testing "try-result-with-context adds context to errors"
    (let [r (result/try-result-with-context
             #(throw (Exception. "File not found"))
             {:file "/tmp/test.txt" :operation :read})]
      (is (result/failure? r))
      (is (= "File not found" (:error r)))
      (is (= "/tmp/test.txt" (-> r :context :file)))
      (is (= :read (-> r :context :operation))))))

(deftest chaining-test
  (testing "Complex chaining scenario"
    (let [parse-int (fn [s]
                      (try
                        (result/ok (Integer/parseInt s))
                        (catch Exception _
                          (result/err "Not a number" {:input s}))))
          validate-positive (fn [n]
                              (if (pos? n)
                                (result/ok n)
                                (result/err "Not positive" {:value n})))
          double-it (fn [n] (result/ok (* 2 n)))

          result (-> (parse-int "42")
                     (result/bind validate-positive)
                     (result/bind double-it))]
      (is (result/success? result))
      (is (= 84 (:value result)))))

  (testing "Chaining with early failure"
    (let [parse-int (fn [s]
                      (try
                        (result/ok (Integer/parseInt s))
                        (catch Exception _
                          (result/err "Not a number" {:input s}))))
          validate-positive (fn [n]
                              (if (pos? n)
                                (result/ok n)
                                (result/err "Not positive" {:value n})))
          double-it (fn [n] (result/ok (* 2 n)))

          result (-> (parse-int "not-a-number")
                     (result/bind validate-positive)
                     (result/bind double-it))]
      (is (result/failure? result))
      (is (= "Not a number" (:error result))))))

;; Property tests for Result composition behavior
;; Verifies that bind/fmap follow expected algebraic properties

(deftest monad-law-left-identity-test
  (testing "Left Identity: bind (ok x) f === f x"
    (let [f (fn [x] (result/ok (* x 2)))
          test-values [0 1 42 -5 1000]]
      (doseq [x test-values]
        (let [left (result/bind (result/ok x) f)
              right (f x)]
          (is (= left right)
              (str "Left identity failed for value: " x))))))

  (testing "Left Identity with failure-producing function"
    (let [f (fn [x] (if (pos? x)
                      (result/ok x)
                      (result/err "negative" {})))
          test-values [-5 -1 0 1 5]]
      (doseq [x test-values]
        (let [left (result/bind (result/ok x) f)
              right (f x)]
          (is (= left right)
              (str "Left identity failed for value: " x)))))))

(deftest monad-law-right-identity-test
  (testing "Right Identity: bind m ok === m"
    (let [test-results [(result/ok 42)
                        (result/ok 0)
                        (result/ok "hello")
                        (result/ok [1 2 3])
                        (result/err "error" {})
                        (result/err "failed" {:code 404})]]
      (doseq [m test-results]
        (let [left (result/bind m result/ok)
              right m]
          (is (= left right)
              (str "Right identity failed for: " m)))))))

(deftest monad-law-associativity-test
  (testing "Associativity: bind (bind m f) g === bind m (λx. bind (f x) g)"
    (let [f (fn [x] (result/ok (* x 2)))
          g (fn [x] (result/ok (+ x 10)))
          test-values [0 1 5 42 -3]]
      (doseq [x test-values]
        (let [m (result/ok x)
              ;; Left side: bind (bind m f) g
              left (result/bind (result/bind m f) g)
              ;; Right side: bind m (λx. bind (f x) g)
              right (result/bind m (fn [y] (result/bind (f y) g)))]
          (is (= left right)
              (str "Associativity failed for value: " x))))))

  (testing "Associativity with mixed success/failure"
    (let [f (fn [x] (if (even? x)
                      (result/ok (/ x 2))
                      (result/err "odd number" {:value x})))
          g (fn [x] (result/ok (* x 3)))
          test-values [2 4 6 3 5 7]]
      (doseq [x test-values]
        (let [m (result/ok x)
              left (result/bind (result/bind m f) g)
              right (result/bind m (fn [y] (result/bind (f y) g)))]
          (is (= left right)
              (str "Associativity failed for value: " x))))))

  (testing "Associativity with initial failure"
    (let [f (fn [x] (result/ok (* x 2)))
          g (fn [x] (result/ok (+ x 10)))
          m (result/err "initial failure" {})
          left (result/bind (result/bind m f) g)
          right (result/bind m (fn [y] (result/bind (f y) g)))]
      (is (= left right)
          "Associativity should hold even with initial failure"))))

(deftest monad-law-functor-composition-test
  (testing "Functor law: fmap (f ∘ g) === fmap f ∘ fmap g"
    (let [f (fn [x] (* x 2))
          g (fn [x] (+ x 10))
          test-values [0 1 5 42 -3]]
      (doseq [x test-values]
        (let [m (result/ok x)
              ;; Left side: fmap (f ∘ g)
              left (result/fmap m (comp f g))
              ;; Right side: (fmap f) ∘ (fmap g)
              right (-> m (result/fmap g) (result/fmap f))]
          (is (= left right)
              (str "Functor composition failed for value: " x))))))

  (testing "Functor preserves identity"
    (let [test-values [(result/ok 42)
                       (result/ok "hello")
                       (result/err "error" {})]]
      (doseq [m test-values]
        (let [left (result/fmap m identity)
              right m]
          (is (= left right)
              (str "Functor identity failed for: " m)))))))

(deftest monad-consistency-with-functor-test
  (testing "fmap f m === bind m (ok ∘ f)"
    (let [f (fn [x] (* x 2))
          test-values [0 1 5 42 -3]]
      (doseq [x test-values]
        (let [m (result/ok x)
              left (result/fmap m f)
              right (result/bind m (comp result/ok f))]
          (is (= left right)
              (str "Monad-functor consistency failed for value: " x)))))))

;; Tests for new Result combinators

(deftest traverse-test
  (testing "traverse applies Result-returning function to collection"
    (let [parse-int (fn [s]
                      (try
                        (result/ok (Integer/parseInt s))
                        (catch Exception _
                          (result/err "Not a number" {:input s}))))
          inputs ["1" "2" "3"]
          result (result/traverse parse-int inputs)]
      (is (result/success? result))
      (is (= [1 2 3] (:value result)))))

  (testing "traverse fails on first error"
    (let [parse-int (fn [s]
                      (try
                        (result/ok (Integer/parseInt s))
                        (catch Exception _
                          (result/err "Not a number" {:input s}))))
          inputs ["1" "bad" "3"]
          result (result/traverse parse-int inputs)]
      (is (result/failure? result))
      (is (= "Not a number" (:error result)))
      (is (= "bad" (-> result :context :input)))))

  (testing "traverse works with empty collection"
    (let [f (fn [x] (result/ok (* x 2)))
          result (result/traverse f [])]
      (is (result/success? result))
      (is (= [] (:value result)))))

  (testing "traverse preserves order"
    (let [f (fn [x] (result/ok (* x 2)))
          inputs [1 2 3 4 5]
          result (result/traverse f inputs)]
      (is (result/success? result))
      (is (= [2 4 6 8 10] (:value result))))))

(deftest sequence-results-test
  (testing "sequence-results converts collection of Results to Result of collection"
    (let [results [(result/ok 1) (result/ok 2) (result/ok 3)]
          sequenced (result/sequence-results results)]
      (is (result/success? sequenced))
      (is (= [1 2 3] (:value sequenced)))))

  (testing "sequence-results fails on first error"
    (let [results [(result/ok 1)
                   (result/err "Error 2" {})
                   (result/ok 3)
                   (result/err "Error 4" {})]
          sequenced (result/sequence-results results)]
      (is (result/failure? sequenced))
      (is (= "Error 2" (:error sequenced)))))

  (testing "sequence-results works with empty collection"
    (let [sequenced (result/sequence-results [])]
      (is (result/success? sequenced))
      (is (= [] (:value sequenced)))))

  (testing "sequence-results preserves order"
    (let [results [(result/ok 10) (result/ok 20) (result/ok 30)]
          sequenced (result/sequence-results results)]
      (is (result/success? sequenced))
      (is (= [10 20 30] (:value sequenced))))))

(deftest map-err-test
  (testing "map-err transforms error message in Failure"
    (let [r (result/err "file not found" {:path "/tmp/test.txt"})
          transformed (result/map-err r #(str "ERROR: " %))]
      (is (result/failure? transformed))
      (is (= "ERROR: file not found" (:error transformed)))
      (is (= "/tmp/test.txt" (-> transformed :context :path)))))

  (testing "map-err preserves context"
    (let [r (result/err "timeout" {:duration 5000 :operation :fetch})
          transformed (result/map-err r clojure.string/upper-case)]
      (is (result/failure? transformed))
      (is (= "TIMEOUT" (:error transformed)))
      (is (= 5000 (-> transformed :context :duration)))
      (is (= :fetch (-> transformed :context :operation)))))

  (testing "map-err does nothing to Success"
    (let [r (result/ok 42)
          transformed (result/map-err r #(str "Should not apply: " %))]
      (is (result/success? transformed))
      (is (= 42 (:value transformed)))))

  (testing "map-err can transform to structured error"
    (let [r (result/err "simple error" {})
          transformed (result/map-err r (fn [msg] {:type :user-error :message msg}))]
      (is (result/failure? transformed))
      (is (= {:type :user-error :message "simple error"} (:error transformed))))))

(deftest tap-test
  (testing "tap executes side effect on Success and returns original"
    (let [captured (atom nil)
          r (result/ok 42)
          tapped (result/tap r (fn [v] (reset! captured v)))]
      (is (result/success? tapped))
      (is (= 42 (:value tapped)))
      (is (= 42 @captured))
      (is (= r tapped) "tap should return identical result")))

  (testing "tap does not execute on Failure"
    (let [captured (atom :not-called)
          r (result/err "failed" {})
          tapped (result/tap r (fn [v] (reset! captured v)))]
      (is (result/failure? tapped))
      (is (= "failed" (:error tapped)))
      (is (= :not-called @captured) "Side effect should not execute on Failure")))

  (testing "tap preserves value in chain"
    (let [side-effects (atom [])
          result (-> (result/ok 5)
                     (result/tap (fn [v] (swap! side-effects conj [:first v])))
                     (result/fmap #(* % 2))
                     (result/tap (fn [v] (swap! side-effects conj [:second v])))
                     (result/fmap inc))]
      (is (result/success? result))
      (is (= 11 (:value result)))
      (is (= [[:first 5] [:second 10]] @side-effects))))

  (testing "tap can be used for logging without breaking chain"
    (let [log (atom [])
          result (-> (result/ok "data")
                     (result/tap (fn [v] (swap! log conj (str "Processing: " v))))
                     (result/fmap clojure.string/upper-case)
                     (result/tap (fn [v] (swap! log conj (str "Result: " v)))))]
      (is (result/success? result))
      (is (= "DATA" (:value result)))
      (is (= ["Processing: data" "Result: DATA"] @log)))))

(deftest result-or-exit-test
  (testing "result-or-exit returns value on Success"
    (let [r (result/ok 42)
          value (result/result-or-exit r)]
      (is (= 42 value))))

  (testing "result-or-exit exits on Failure"
    (let [r (result/err "Something went wrong" {:file "/tmp/test.txt"})
          print-called (atom false)
          exit-called (atom nil)]
      (binding [result/*exit-fn* (fn [code] (reset! exit-called code))]
        (with-redefs [result/print-failure (fn [res]
                                             (reset! print-called true)
                                             (is (= "Something went wrong" (:error res))))]
          (result/result-or-exit r)))
      (is @print-called "print-failure should be called")
      (is (= 2 @exit-called) "exit function should be called with the error code, 2")))

  (testing "result-or-exit with custom exit code"
    (let [r (result/err "Permission denied" {:code 403})
          exit-code (atom nil)]
      (binding [result/*exit-fn* (fn [code] (reset! exit-code code))]
        (with-redefs [result/print-failure (fn [_] nil)]
          (result/result-or-exit r 403)))
      (is (= 403 @exit-code)))))
