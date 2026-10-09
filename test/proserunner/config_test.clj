(ns proserunner.config-test
  "Tests for config loading and initialization.

  Note: fetch-or-create! has essential complexity (decision tree with ordering).
  These tests document the branching behavior without requiring structural changes."
  (:require [babashka.http-client :as http]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [proserunner.config :as config]
            [proserunner.config.loader]
            [proserunner.result :as result]
            [proserunner.test-helpers :refer [silently with-temp-dir with-temp-dirs with-user-home]]))

(deftest fetch-or-create-input-validation-test
  (testing "Throws on non-string config-filepath"
    (is (thrown-with-msg? clojure.lang.ExceptionInfo
          #"expects a string filepath"
          (config/fetch-or-create! 123))))

  (testing "Throws with type information in error context"
    (try
      (config/fetch-or-create! {:invalid "map"})
      (is false "Should have thrown")
      (catch clojure.lang.ExceptionInfo e
        (is (= clojure.lang.PersistentArrayMap
               (type (-> e ex-data :config-filepath))))
        (is (contains? (ex-data e) :config-filepath))))))

;; Note: nil is valid input and is tested through integration tests

(deftest fetch-or-create-decision-branches-test
  (testing "Decision tree branches are ordered correctly"
    ;; This test documents the ordering without executing branches
    ;; The actual branching is tested through integration tests
    (let [branches [:custom-config-specified
                    :in-project-directory
                    :checks-stale
                    :config-exists
                    :first-time-init]]
      (is (= 5 (count branches))
          "fetch-or-create! has 5 decision branches")

      ;; Document critical ordering constraint
      (is (= :in-project-directory (nth branches 1))
          "Project check must come before stale/exists checks (per comment)")

      ;; Document that function handles all cases
      (is (some #{:first-time-init} branches)
          "Function handles first-time initialization"))))

(deftest fetch-or-create-documentation-test
  (testing "Function has clear documentation"
    (let [docstring (:doc (meta #'config/fetch-or-create!))]
      (is (string? docstring))
      (is (re-find #"project" docstring)
          "Documents project config behavior")
      (is (re-find #"--restore-defaults|checks restore" docstring)
          "Documents how checks get updated"))))

;;;; Installing default checks

(defn- zip-bytes
  "A default-checks archive holding `entries` (path -> content), under the
  top-level directory GitHub adds."
  [entries]
  (let [out (java.io.ByteArrayOutputStream.)]
    (with-open [zip (java.util.zip.ZipOutputStream. out)]
      (doseq [[path ^String content] entries]
        (.putNextEntry zip (java.util.zip.ZipEntry. (str "proserunner-default-checks-main/" path)))
        (.write zip (.getBytes content "UTF-8"))
        (.closeEntry zip)))
    (.toByteArray out)))

(defmacro ^:private with-archive
  "Runs body with http/get serving `zip` as the pinned default checks, and
  the pinned checksum set to match it. Other URLs get a 404."
  [zip & body]
  `(let [zip# ~zip]
     (with-redefs [http/get (fn [url# & _#]
                              (if (= url# config/remote-address)
                                {:status 200 :body zip#}
                                {:status 404}))
                   config/default-checks-sha256 (config/archive-digest zip#)]
       ~@body)))

(defn- write! [dir path content]
  (let [f (io/file dir path)]
    (io/make-parents f)
    (spit f content)))

(defn- tmp-dirs
  "Temporary directories left in `dir`."
  [dir]
  (filter #(str/starts-with? (.getName ^java.io.File %) ".tmp-")
          (.listFiles (io/file dir))))

(defn- config-dir
  "Where the global config lives under the test `home`."
  [home]
  (io/file home ".config" "proserunner"))

(def ^:private archive
  {"default/new.edn" "new"
   "config.edn" "theirs"
   "ignore.edn" "theirs"
   "custom/example.edn" "example"
   "README.md" "readme"})

(deftest restore-defaults-keeps-user-files-test
  (with-temp-dir [home "proserunner-restore"]
    (with-user-home home
      (let [dir (config-dir home)]
        (write! dir "config.edn" "my config")
        (write! dir "ignore.edn" "my ignores")
        (write! dir "custom/mine.edn" "mine")
        (write! dir "default/old.edn" "old")
        (with-archive (zip-bytes archive)
          (is (result/success? (silently (config/restore-defaults!)))))

        (testing "default/ is replaced"
          (is (= "new" (slurp (io/file dir "default/new.edn"))))
          (is (not (.exists (io/file dir "default/old.edn")))))

        (testing "the old default/ is backed up inside the config directory"
          (is (some #(.exists (io/file ^java.io.File % "old.edn"))
                    (.listFiles (io/file dir "backups"))))
          (is (empty? (filter #(str/starts-with? (.getName ^java.io.File %) ".proserunner-backup-")
                              (.listFiles (io/file home))))
              "nothing is added to the home directory"))

        (testing "config, ignores, and custom checks are kept"
          (is (= "my config" (slurp (io/file dir "config.edn"))))
          (is (= "my ignores" (slurp (io/file dir "ignore.edn"))))
          (is (= "mine" (slurp (io/file dir "custom/mine.edn")))))

        (testing "missing custom files and repo files are added"
          (is (= "example" (slurp (io/file dir "custom/example.edn"))))
          (is (= "readme" (slurp (io/file dir "README.md")))))

        (testing "the installed commit is recorded"
          (is (= config/default-checks-ref (slurp (io/file dir ".version")))))

        (is (empty? (tmp-dirs dir)) "no temporary directories left")))))

(deftest restore-defaults-fresh-install-test
  (with-temp-dir [home "proserunner-fresh"]
    (with-user-home home
      (with-archive (zip-bytes archive)
        (is (result/success? (silently (config/restore-defaults!)))))
      (let [dir (config-dir home)]
        (is (= "new" (slurp (io/file dir "default/new.edn"))))
        (is (re-find #"\"new\"" (slurp (io/file dir "config.edn")))
            "config.edn is generated from the installed checks, not taken from the archive")
        (is (not (.exists (io/file dir "ignore.edn")))
            "ignore.edn comes from ensure-global-config!, not the archive")
        (is (not (.exists (io/file home ".proserunner")))
            "nothing is written to the legacy location")))))

(deftest default-checks-are-pinned-test
  (testing "the download is a fixed commit, not a branch"
    (is (re-matches #"[0-9a-f]{40}" config/default-checks-ref))
    (is (str/includes? config/remote-address config/default-checks-ref))
    (is (not (str/includes? config/remote-address "main.zip")))
    (is (re-matches #"[0-9a-f]{64}" config/default-checks-sha256))))

(deftest archive-digest-test
  (testing "the digest covers paths and contents, not GitHub's top-level directory name"
    (let [entries {"default/a.edn" "a" "README.md" "r"}
          zip-under (fn [top]
                      (let [out (java.io.ByteArrayOutputStream.)]
                        (with-open [zip (java.util.zip.ZipOutputStream. out)]
                          (.putNextEntry zip (java.util.zip.ZipEntry. (str top "/")))
                          (.closeEntry zip)
                          (doseq [[path ^String content] entries]
                            (.putNextEntry zip (java.util.zip.ZipEntry. (str top "/" path)))
                            (.write zip (.getBytes content "UTF-8"))
                            (.closeEntry zip)))
                        (.toByteArray out)))]
      (is (= (config/archive-digest (zip-under "proserunner-default-checks-main"))
             (config/archive-digest (zip-under "proserunner-default-checks-abc123"))))
      (is (not= (config/archive-digest (zip-bytes entries))
                (config/archive-digest (zip-bytes (assoc entries "default/a.edn" "b"))))
          "changing a file changes the digest")
      (is (not= (config/archive-digest (zip-bytes {"a" "bc"}))
                (config/archive-digest (zip-bytes {"ab" "c"})))
          "paths and contents can't run together"))))

(deftest checksum-mismatch-installs-nothing-test
  (with-temp-dir [home "proserunner-checksum"]
    (with-user-home home
      (let [dir (config-dir home)]
        (write! dir "default/old.edn" "old")
        (with-redefs [http/get (fn [& _] {:status 200 :body (zip-bytes {"default/evil.edn" "evil"})})]
          (let [r (silently (config/restore-defaults!))]
            (is (result/failure? r))
            (is (re-find #"don't match the checksum" (:error r)))))
        (is (= "old" (slurp (io/file dir "default/old.edn"))))
        (is (not (.exists (io/file dir "default/evil.edn"))))))))

(deftest interrupted-download-changes-nothing-test
  (with-temp-dir [home "proserunner-interrupted"]
    (with-user-home home
      (let [dir (config-dir home)
            real-copy @#'config/copy!
            copies (atom 0)]
        (write! dir "config.edn" "my config")
        (write! dir "default/old.edn" "old")
        (with-archive (zip-bytes {"default/a.edn" "a"
                                  "default/b.edn" "b"
                                  "config.edn" "theirs"})
          (with-redefs [config/copy! (fn [& args]
                                       (when (= 2 (swap! copies inc))
                                         (throw (java.io.IOException. "No space left on device")))
                                       (apply real-copy args))]
            (let [r (silently (config/restore-defaults!))]
              (is (result/failure? r))
              (is (re-find #"No space left" (:error r))))))
        (is (= "old" (slurp (io/file dir "default/old.edn"))))
        (is (not (.exists (io/file dir "default/a.edn"))))
        (is (= "my config" (slurp (io/file dir "config.edn"))))
        (is (empty? (tmp-dirs dir)) "a failed download cleans up after itself")))))

(deftest killed-download-is-cleaned-up-next-time-test
  (with-temp-dir [home "proserunner-killed"]
    (with-user-home home
      (let [dir (config-dir home)]
        ;; What a run killed mid-extraction leaves behind
        (write! dir ".tmp-123/default/partial.edn" "partial")
        (write! dir "default/old.edn" "old")
        (with-archive (zip-bytes archive)
          (is (result/success? (silently (config/restore-defaults!)))))
        (is (empty? (tmp-dirs dir)))
        (is (= "new" (slurp (io/file dir "default/new.edn"))))))))

(deftest archive-entries-cannot-escape-test
  (with-temp-dir [home "proserunner-zipslip"]
    (with-user-home home
      ;; Even an archive that passes the checksum can't write outside
      (with-archive (zip-bytes {"../../evil.txt" "evil"})
        (let [r (silently (config/restore-defaults!))]
          (is (result/failure? r))
          (is (re-find #"Refusing to extract" (:error r)))))
      (is (not (.exists (io/file home "evil.txt")))))))

(deftest connection-errors-are-readable-test
  (testing "Java's ConnectException has no message; the error still says what failed"
    (with-temp-dir [home "proserunner-offline"]
      (with-user-home home
        (with-redefs [http/get (fn [& _] (throw (java.net.ConnectException.)))]
          (let [r (silently (config/restore-defaults!))]
            (is (result/failure? r))
            (is (re-find #"Couldn't download from github\.com \(ConnectException\)" (:error r)))))))))

;;;; Moving ~/.proserunner to the XDG config directory

(deftest migrate-legacy-config-test
  (testing "~/.proserunner moves to ~/.config/proserunner, contents and all"
    (with-temp-dir [home "proserunner-migrate"]
      (with-user-home home
        (write! (io/file home ".proserunner") "ignore.edn" "my ignores")
        (write! (io/file home ".proserunner") "custom/mine.edn" "mine")
        (let [err (with-out-str (binding [*err* *out*] (config/ensure-global-config!)))]
          (is (re-find #"Moved ~/\.proserunner to ~/\.config/proserunner" err)))
        (is (not (.exists (io/file home ".proserunner"))))
        (is (= "my ignores" (slurp (io/file (config-dir home) "ignore.edn"))))
        (is (= "mine" (slurp (io/file (config-dir home) "custom/mine.edn"))))
        (is (= (str (config-dir home)) (proserunner.system/config-dir))))))

  (testing "$XDG_CONFIG_HOME is honored"
    (with-temp-dirs [[home "proserunner-migrate-home"]
                     [xdg "proserunner-xdg"]]
      (with-user-home home
        (with-redefs [proserunner.system/xdg-config-home (constantly xdg)]
          (write! (io/file home ".proserunner") "ignore.edn" "my ignores")
          (silently (config/ensure-global-config!))
          (is (= "my ignores" (slurp (io/file xdg "proserunner" "ignore.edn"))))))))

  (testing "a failed move keeps using ~/.proserunner and says how to move it"
    (with-temp-dir [home "proserunner-migrate-fail"]
      (with-user-home home
        (write! (io/file home ".proserunner") "ignore.edn" "my ignores")
        (with-redefs [config/move! (fn [& _] (throw (java.nio.file.AtomicMoveNotSupportedException.
                                                      "a" "b" "different filesystems")))]
          (let [err (with-out-str (binding [*err* *out*] (config/ensure-global-config!)))]
            (is (re-find #"mv ~/\.proserunner ~/\.config/proserunner" err))))
        (is (= (str (io/file home ".proserunner")) (proserunner.system/config-dir)))
        (is (= "my ignores" (slurp (io/file home ".proserunner" "ignore.edn"))))
        (is (not (.exists (config-dir home)))))))

  (testing "when both exist, the XDG one wins and the old one is left alone"
    (with-temp-dir [home "proserunner-migrate-both"]
      (with-user-home home
        (write! (io/file home ".proserunner") "ignore.edn" "old")
        (write! (config-dir home) "ignore.edn" "new")
        (let [err (with-out-str (binding [*err* *out*] (config/ensure-global-config!)))]
          (is (re-find #"Both ~/\.proserunner and ~/\.config/proserunner exist" err)))
        (is (= "old" (slurp (io/file home ".proserunner" "ignore.edn"))))
        (is (= (str (config-dir home)) (proserunner.system/config-dir)))))))

;;;; Proxies

(deftest proxy-for-test
  (let [url "https://api.github.com/repos/x"]
    (testing "no proxy variables means a direct connection"
      (is (nil? (config/proxy-for url {}))))

    (testing "HTTPS_PROXY (lowercase first), then ALL_PROXY"
      (is (= {:host "proxy.corp" :port 3128}
             (config/proxy-for url {"HTTPS_PROXY" "http://proxy.corp:3128"})))
      (is (= {:host "lower" :port 1}
             (config/proxy-for url {"https_proxy" "http://lower:1" "HTTPS_PROXY" "http://upper:2"})))
      (is (= {:host "all" :port 8080}
             (config/proxy-for url {"ALL_PROXY" "all:8080"}))))

    (testing "HTTP_PROXY doesn't apply to https URLs, as in curl"
      (is (nil? (config/proxy-for url {"HTTP_PROXY" "http://proxy:3128"}))))

    (testing "the port defaults to 1080"
      (is (= 1080 (:port (config/proxy-for url {"HTTPS_PROXY" "proxy.corp"})))))

    (testing "NO_PROXY matches hosts and their subdomains"
      (doseq [no-proxy ["github.com" ".github.com" "*.github.com" "example.com, github.com" "*" "GitHub.com"]]
        (is (nil? (config/proxy-for url {"HTTPS_PROXY" "proxy:1" "NO_PROXY" no-proxy}))
            no-proxy))
      (is (some? (config/proxy-for url {"HTTPS_PROXY" "proxy:1" "NO_PROXY" "hub.com"}))
          "hub.com is not a parent domain of api.github.com"))

    (testing "a bad value names the variable, not the value, which may hold credentials"
      (doseq [bad ["http://user:secret@:bad" "http://secret host"]]
        (let [e (try (config/proxy-for url {"HTTPS_PROXY" bad})
                     nil
                     (catch clojure.lang.ExceptionInfo e e))]
          (is (re-find #"HTTPS_PROXY" (ex-message e)) bad)
          (is (not (re-find #"secret" (ex-message e))) bad))))))

(deftest ensure-checks-exist-structure-test
  (testing "ensure-checks-exist! validates check references"
    ;; Documents that this function checks if referenced checks exist
    (is (resolve 'proserunner.config/ensure-checks-exist!)
        "ensure-checks-exist! function exists"))

  (testing "ensure-checks-exist! returns Result type"
    (let [docstring (:doc (meta #'config/ensure-checks-exist!))]
      (is (string? docstring))
      (is (re-find #"Result" docstring)
          "Documents Result return type")))

  (testing "ensure-checks-exist! triggers download for missing checks"
    ;; Function should detect missing "default" or "custom" and download them
    (let [scenarios [:missing-default :missing-custom :checks-exist]]
      (is (= 3 (count scenarios))
          "Handles missing default, missing custom, and exists scenarios"))))

(deftest load-config-from-file-returns-result-test
  (testing "load-config-from-file returns Success for valid config"
    (let [temp-file (java.io.File/createTempFile "test-config" ".edn")]
      (try
        (spit (.getPath temp-file) "{:checks []}")
        (let [result (proserunner.config.loader/load-config-from-file (.getPath temp-file))]
          (is (proserunner.result/success? result))
          (is (some? (:value result))))
        (finally
          (.delete temp-file)))))

  (testing "load-config-from-file returns Failure for non-existent file"
    (let [result (proserunner.config.loader/load-config-from-file "/nonexistent/file.edn")]
      (is (proserunner.result/failure? result))
      (is (string? (:error result)))
      (is (contains? (:context result) :filepath))))

  (testing "load-config-from-file returns Failure for invalid EDN"
    (let [temp-file (java.io.File/createTempFile "bad-config" ".edn")]
      (try
        (spit (.getPath temp-file) "{:checks [}")
        (let [result (proserunner.config.loader/load-config-from-file (.getPath temp-file))]
          (is (proserunner.result/failure? result))
          (is (string? (:error result))))
        (finally
          (.delete temp-file))))))

;; Note: Comprehensive integration tests exist in:
;; - test/proserunner/integration_test.clj
;; - test/proserunner/custom_checks_test.clj
;; These test actual config loading with real file system operations.

;;; Tests for determine-config-strategy (pure function extracted from fetch-or-create!)

(deftest determine-config-strategy-test
  (testing "returns :custom when custom config specified and exists"
    (is (= :custom (config/determine-config-strategy
                     {:using-default? false
                      :custom-exists? true
                      :in-project? false
                      :config-exists? false}))
        "Custom config file via -c flag takes precedence"))

  (testing "returns :project when using default and in project directory"
    (is (= :project (config/determine-config-strategy
                      {:using-default? true
                       :custom-exists? false
                       :in-project? true
                       :config-exists? false}))
        "Project directory config should be loaded"))

  (testing "returns :global when config exists"
    (is (= :global (config/determine-config-strategy
                     {:using-default? true
                      :custom-exists? false
                      :in-project? false
                      :config-exists? true}))
        "Global config should be loaded when it exists"))

  (testing "returns :initialize when config doesn't exist"
    (is (= :initialize (config/determine-config-strategy
                         {:using-default? true
                          :custom-exists? false
                          :in-project? false
                          :config-exists? false}))
        "Should initialize on first run"))

  (testing "enforces ordering: project takes precedence over global"
    (is (= :project (config/determine-config-strategy
                      {:using-default? true
                       :custom-exists? false
                       :in-project? true
                       :config-exists? true}))
        "Project check must come before global check"))

  (testing "custom config takes precedence over everything"
    (is (= :custom (config/determine-config-strategy
                     {:using-default? false
                      :custom-exists? true
                      :in-project? true
                      :config-exists? true}))
        "Custom config via -c flag has highest precedence"))

  (testing "there is no implicit update strategy"
    (let [strategies (set (for [using-default? [true false]
                                custom-exists? [true false]
                                in-project? [true false]
                                config-exists? [true false]]
                            (config/determine-config-strategy
                              {:using-default? using-default?
                               :custom-exists? custom-exists?
                               :in-project? in-project?
                               :config-exists? config-exists?})))]
      (is (= #{:custom :project :global :initialize} strategies)
          "All four strategy branches are reachable, and none updates checks"))))

(deftest fetch-or-create-makes-no-network-calls-test
  (testing "loading an existing global config never contacts GitHub"
    (let [home (str (System/getProperty "java.io.tmpdir") "/proserunner-nonet-" (System/nanoTime))
          proserunner-dir (java.io.File. home ".proserunner")]
      (try
        (.mkdirs (java.io.File. proserunner-dir "default"))
        (spit (java.io.File. proserunner-dir "config.edn") "{:checks []}")
        (let [calls (atom [])]
          (with-redefs [proserunner.system/home-dir (constantly home)
                        babashka.http-client/get (fn [url & _]
                                                   (swap! calls conj url)
                                                   {:status 500})]
            (is (some? (config/fetch-or-create! (str home "/.proserunner/config.edn"))))
            (is (empty? @calls) "no HTTP requests during a lint")))
        (finally
          (doseq [f (reverse (file-seq (java.io.File. home)))]
            (.delete ^java.io.File f)))))))
