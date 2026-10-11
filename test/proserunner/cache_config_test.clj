(ns proserunner.cache-config-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.cache-config :as cache]))

(deftest test-resolve-cache-path-cli-priority
  (testing "CLI flag takes highest priority"
    (is (= "/cli/cache"
           (cache/resolve-cache-path
             {:cli-cache-dir "/cli/cache"
              :env-vars {"PROSERUNNER_CACHE_DIR" "/env/cache"
                        "XDG_CACHE_HOME" "/xdg"}
              :system-props {"java.io.tmpdir" "/tmp"}}))))

  (testing "CLI overrides everything even when others present"
    (is (= "/custom"
           (cache/resolve-cache-path
             {:cli-cache-dir "/custom"
              :env-vars {"PROSERUNNER_CACHE_DIR" "/env"}
              :system-props {"java.io.tmpdir" "/tmp"}})))))

(deftest test-resolve-cache-path-env-priority
  (testing "PROSERUNNER_CACHE_DIR when no CLI"
    (is (= "/env/cache"
           (cache/resolve-cache-path
             {:env-vars {"PROSERUNNER_CACHE_DIR" "/env/cache"
                        "XDG_CACHE_HOME" "/xdg"}
              :system-props {"java.io.tmpdir" "/tmp"}}))))

  (testing "env overrides XDG and tmpdir"
    (is (= "/env"
           (cache/resolve-cache-path
             {:env-vars {"PROSERUNNER_CACHE_DIR" "/env"
                        "XDG_CACHE_HOME" "/xdg"}
              :system-props {"java.io.tmpdir" "/tmp"}})))))

(deftest test-resolve-cache-path-xdg-fallback
  (testing "XDG_CACHE_HOME when no CLI or env"
    (is (= "/xdg/proserunner"
           (cache/resolve-cache-path
             {:env-vars {"XDG_CACHE_HOME" "/xdg"}
              :system-props {"java.io.tmpdir" "/tmp"}}))))

  (testing "XDG appends /proserunner"
    (is (= "/home/user/.cache/proserunner"
           (cache/resolve-cache-path
             {:env-vars {"XDG_CACHE_HOME" "/home/user/.cache"}
              :system-props {"java.io.tmpdir" "/tmp"}})))))

(deftest test-resolve-cache-path-tmpdir-fallback
  (testing "tmpdir when no other options"
    (is (= "/tmp/proserunner-storage"
           (cache/resolve-cache-path
             {:system-props {"java.io.tmpdir" "/tmp"}}))))

  (testing "tmpdir appends /proserunner-storage"
    (is (= "/var/tmp/proserunner-storage"
           (cache/resolve-cache-path
             {:system-props {"java.io.tmpdir" "/var/tmp"}})))))

(deftest test-resolve-cache-path-empty-maps
  (testing "handles nil env-vars"
    (is (= "/tmp/proserunner-storage"
           (cache/resolve-cache-path
             {:system-props {"java.io.tmpdir" "/tmp"}}))))

  (testing "handles empty env-vars map"
    (is (= "/tmp/proserunner-storage"
           (cache/resolve-cache-path
             {:env-vars {}
              :system-props {"java.io.tmpdir" "/tmp"}})))))

(deftest test-make-cache-file-path
  (testing "constructs correct path"
    (is (= "/tmp/cache/fileabc123.edn"
           (cache/make-cache-file-path "/tmp/cache" "abc123"))))

  (testing "handles different cache dirs"
    (is (= "/custom/location/file999.edn"
           (cache/make-cache-file-path "/custom/location" "999"))))

  (testing "works with absolute paths"
    (is (= "/home/user/.cache/proserunner/filehash.edn"
           (cache/make-cache-file-path "/home/user/.cache/proserunner" "hash")))))
