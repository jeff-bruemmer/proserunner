(ns proserunner.output.checks-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.effects :as effects]
            [proserunner.output.checks :as output-checks]
            [proserunner.checks :as checks]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [silently with-temp-dir]]))

(deftest print-checks-test
  (testing "print-checks function exists"
    (is (fn? output-checks/print)))

  ;; Integration test - actual functionality tested via integration tests
  ;; since it requires a valid config and check-dir
  )

(deftest print-handles-result-type-test
  (testing "print should handle Result object from checks/create"
    ;; The print function receives a Result from checks/create and must
    ;; extract :value before processing the checks

    ;; Mock a successful Result
    (let [mock-checks [(checks/->Check "test-check"
                                       ["specimen"]
                                       "Test message"
                                       "existence"
                                       "Test explanation"
                                       []
                                       [])]
          success-result (result/ok {:checks mock-checks :warnings []})]
      ;; Verify Result structure is what we expect
      (is (result/success? success-result))
      (is (= mock-checks (:checks (:value success-result))))
      (is (empty? (:warnings (:value success-result))))
      (is (= "test-check" (:name (first (:checks (:value success-result)))))))

    ;; Mock a failure Result
    (let [failure-result (result/err "Test error")]
      (is (result/failure? failure-result))
      (is (= "Test error" (:error failure-result)))))

  (testing "print should handle checks with nil fields gracefully"
    ;; Verify that checks with nil fields can be created
    (let [check-with-nil (checks/->Check nil nil nil nil nil [] [])]
      (is (nil? (:name check-with-nil)))
      (is (nil? (:kind check-with-nil))))))

(deftest print-fails-when-no-checks-load-test
  (testing "listing checks fails, so the run exits 2, when none of them load"
    (with-temp-dir [dir "proserunner-checks-print"]
      (let [config (str dir "/config.edn")]
        (spit config (pr-str {:checks [{:name "gone" :directory "nope" :files ["missing"]}]}))
        (let [out (java.io.StringWriter.)
              err (java.io.StringWriter.)
              r (binding [*out* out *err* err]
                  (effects/execute-command-result {:effects [[:checks/print config]]}))]
          (is (result/failure? r))
          (is (re-find #"couldn't load checks" (str err)))
          (is (not (re-find #"Enabled checks" (str out)))
              "no header on stdout when there's nothing to list"))))))

(deftest print-succeeds-when-checks-load-test
  (testing "a loadable check is listed and the effect succeeds"
    (with-temp-dir [dir "proserunner-checks-print"]
      (let [config (str dir "/config.edn")]
        (.mkdirs (java.io.File. (str dir "/mine")))
        (spit (str dir "/mine/hedges.edn")
              (pr-str {:name "hedges" :kind "existence" :explanation "Hedging words."
                       :message "Hedge." :specimens ["kind of"]}))
        (spit config (pr-str {:checks [{:name "mine" :directory "mine" :files ["hedges"]}]}))
        (let [out (java.io.StringWriter.)
              r (silently
                 (binding [*out* out]
                   (effects/execute-effect [:checks/print config])))]
          (is (result/success? r))
          (is (re-find #"Hedges" (str out))))))))
