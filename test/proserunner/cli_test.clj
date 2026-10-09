(ns proserunner.cli-test
  "Tests for argument parsing: commands, short flags, and deprecated flags."
  (:require [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [proserunner.cli :as cli]))

(defn- parse [& args] (cli/parse args))

(deftest commands-set-the-same-options-as-the-old-flags-test
  (doseq [[args old-args] [[["ignore" "add" "hopefully"] ["--add-ignore" "hopefully"]]
                           [["ignore" "remove" "hopefully"] ["--remove-ignore" "hopefully"]]
                           [["ignore" "rm" "hopefully"] ["--remove-ignore" "hopefully"]]
                           [["ignore" "list"] ["--list-ignored"]]
                           [["ignore" "ls"] ["--list-ignored"]]
                           [["ignore" "clear"] ["--clear-ignored"]]
                           [["ignore" "audit"] ["--audit-ignores"]]
                           [["ignore" "clean"] ["--clean-ignores"]]
                           [["checks"] ["--checks"]]
                           [["checks" "list"] ["--checks"]]
                           [["checks" "add" "dir"] ["--add-checks" "dir"]]
                           [["checks" "restore"] ["--restore-defaults"]]
                           [["init"] ["--init-project"]]]]
    (let [new (apply parse args)
          old (apply parse old-args)]
      (is (empty? (:errors new)) (str args))
      (is (empty? (:warnings new)) (str args " doesn't warn"))
      (is (= (dissoc (:options old) :help-topic)
             (dissoc (:options new) :help-topic))
          (str args " = " old-args))
      (is (seq (:warnings old)) (str old-args " warns")))))

(deftest command-options-can-go-anywhere-test
  (is (= {:add-ignore "hopefully" :global true}
         (select-keys (:options (parse "--global" "ignore" "add" "hopefully")) [:add-ignore :global])))
  (is (= {:add-ignore "hopefully" :global true}
         (select-keys (:options (parse "ignore" "add" "hopefully" "--global")) [:add-ignore :global])))
  (testing "-- lets a specimen start with a dash"
    (is (= "-ly" (:add-ignore (:options (parse "ignore" "add" "--" "-ly")))))))

(deftest paths-test
  (testing "bare paths and check PATH... are the same"
    (is (= ["a.md" "b.md"] (:paths (:options (parse "a.md" "b.md")))))
    (is (= ["a.md" "b.md"] (:paths (:options (parse "check" "a.md" "b.md"))))))
  (testing "--file joins the paths"
    (is (= ["README.md" "a.md"] (:paths (:options (parse "--file" "README.md" "a.md"))))))
  (testing "- is a path, meaning standard input"
    (is (= ["-"] (:paths (:options (parse "-")))))
    (is (= ["-"] (:paths (:options (parse "--file" "-"))))))
  (testing "commands other than check take no paths"
    (is (empty? (:paths (:options (parse "ignore" "list")))))))

(deftest command-errors-test
  (testing "unknown second words list the choices"
    (is (= ["unknown ignore command 'delete'. Commands: add, remove, rm, list, ls, clear, audit, clean."]
           (:errors (parse "ignore" "delete" "x")))))
  (testing "a missing argument shows an example"
    (is (re-find #"'ignore add' needs a SPECIMEN. Example: proserunner ignore add hopefully"
                 (first (:errors (parse "ignore" "add"))))))
  (testing "unquoted phrases aren't silently cut short"
    (is (re-find #"takes one SPECIMEN, but got 2. To ignore a phrase, quote it: proserunner ignore add \"very unique\""
                 (first (:errors (parse "ignore" "add" "very" "unique"))))))
  (testing "extra arguments to commands that take none"
    (is (re-find #"'ignore list' doesn't take arguments, but got: x"
                 (first (:errors (parse "ignore" "list" "x")))))
    (is (seq (:errors (parse "init" "here")))))
  (testing "help for an unknown topic"
    (is (re-find #"no help for 'bogus'" (first (:errors (parse "help" "bogus")))))))

(deftest help-topics-test
  (is (= "ignore" (:help-topic (:options (parse "help" "ignore")))))
  (is (true? (:help (:options (parse "help" "ignore")))))
  (is (= "ignore" (:help-topic (:options (parse "ignore" "add" "x" "-h")))))
  (is (= "checks" (:help-topic (:options (parse "checks" "--help")))))
  (testing "ignore alone shows its help"
    (is (true? (:help (:options (parse "ignore")))))
    (is (= "ignore" (:help-topic (:options (parse "ignore"))))))
  (is (nil? (:help-topic (:options (parse "--help"))))))

(deftest short-flags-test
  (testing "-q means --quiet now, and --quoted-text has no short flag"
    (let [{:keys [options warnings]} (parse "-q" "a.md")]
      (is (true? (:quiet options)))
      (is (false? (:quoted-text options)))
      (is (empty? warnings))))

  (testing "the short flags shown in help"
    (is (= #{"-c" "-e" "-f" "-h" "-o" "-q" "-v"}
           (set (keep first cli/options))))))

(deftest deprecated-short-flags-test
  (testing "retired short flags still work as their long form, with a warning"
    (let [{:keys [options warnings errors]} (parse "-b" "-n" "-d" "/tmp/cache" "-J" "1,3" "a.md")]
      (is (empty? errors))
      (is (true? (:code-blocks options)))
      (is (true? (:no-cache options)))
      (is (= "/tmp/cache" (:cache-dir options)))
      (is (= "1,3" (:ignore-issues options)))
      (is (= #{"-b is deprecated; use '--code-blocks' instead."
               "-n is deprecated; use '--no-cache' instead."
               "-d is deprecated; use '--cache-dir' instead."
               "-J is deprecated; use '--ignore-issues' instead."}
             (set warnings)))
      (is (not-any? #(str/starts-with? (namespace %) "deprecated") (filter namespace (keys options))))))

  (testing "a short flag for a replaced action points to the command, once"
    (let [{:keys [options warnings]} (parse "-A" "hopefully")]
      (is (= "hopefully" (:add-ignore options)))
      (is (= ["-A is deprecated; use 'proserunner ignore add SPECIMEN' instead."] warnings))))

  (testing "clusters of short flags still parse"
    (let [{:keys [options warnings errors]} (parse "-bt" "a.md")]
      (is (empty? errors))
      (is (true? (:code-blocks options)))
      (is (true? (:timer options)))
      (is (= 2 (count warnings)))))

  (testing "-i and --ignore have no effect"
    (is (= ["-i is deprecated and has no effect."] (:warnings (parse "-i" "x" "a.md"))))
    (is (= ["--ignore is deprecated and has no effect."] (:warnings (parse "--ignore" "x" "a.md"))))))

(deftest help-summary-hides-deprecated-flags-test
  (let [shown (set (map :long-opt (:summary (parse))))]
    (is (contains? shown "--quiet"))
    (is (contains? shown "--quoted-text"))
    (is (not-any? shown (keys cli/deprecated-flags)))
    (is (every? :long-opt (:summary (parse))) "no short-only entries")))
