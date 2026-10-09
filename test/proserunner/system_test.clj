(ns proserunner.system-test
  (:require [clojure.test :refer [deftest is testing]]
            [proserunner.system :as sys]))

(deftest display-path-test
  (with-redefs [sys/home-dir (constantly "/home/al")]
    (testing "paths under home start with ~"
      (is (= "~/.config/proserunner/config.edn"
             (sys/display-path "/home/al/.config/proserunner/config.edn")))
      (is (= "~" (sys/display-path "/home/al"))))
    (testing "a trailing separator is kept"
      (is (= "~/.config/proserunner/custom/"
             (sys/display-path "/home/al/.config/proserunner/custom/"))))
    (testing "a directory whose name only starts with home's name isn't under home"
      (is (= "/home/alice/notes.md" (sys/display-path "/home/alice/notes.md"))))
    (testing "paths elsewhere are unchanged"
      (is (= "/tmp/x" (sys/display-path "/tmp/x")))))
  (testing "a home with a trailing separator still matches"
    (with-redefs [sys/home-dir (constantly "/home/al/")]
      (is (= "~/x" (sys/display-path "/home/al/x")))))
  (testing "a root home doesn't turn every path into ~"
    (with-redefs [sys/home-dir (constantly "/")]
      (is (= "/etc/hosts" (sys/display-path "/etc/hosts"))))))
