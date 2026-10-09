(ns proserunner.config
  "Functions for creating a configuration directory."
  (:gen-class)
  (:require [proserunner
             [console :as console]
             [file-utils :as file-utils]
             [result :as result]
             [system :as sys]]
            [proserunner.config.loader :as loader]
            [proserunner.config.manifest :as manifest]
            [proserunner.config.merger :as merger]
            [proserunner.config.types :refer [map->Config]]
            [proserunner.config.check-resolver :as check-resolver]
            [proserunner.project-config :as project-config]
            [babashka.http-client :as client]
            [clojure.string :as string]
            [clojure.java.io :as io]
            [clojure.pprint :as pprint]
            [cheshire.core :as json]))

(set! *warn-on-reflection* true)

(def remote-address "https://github.com/jeff-bruemmer/proserunner-default-checks/archive/main.zip")
(def remote-api "https://api.github.com/repos/jeff-bruemmer/proserunner-default-checks/commits/main")

(def default-global-config-template
  "{:checks []}")

(def default-ignore-template
  "{:ignore #{}\n :ignore-issues #{}}")

;;;; Proxies

(defn- env-entry
  "Returns [name value] for the first of `names` set to a non-blank value in `env`."
  [env names]
  (some #(let [v (get env %)]
           (when-not (string/blank? v) [% v]))
        names))

(defn- no-proxy?
  "True when `host` matches NO_PROXY: a comma-separated list of hosts and
  domains (each also matching its subdomains), or * for every host."
  [host env]
  (let [host (string/lower-case host)
        [_ value] (env-entry env ["no_proxy" "NO_PROXY"])]
    (boolean
     (some (fn [entry]
             (let [domain (string/replace (string/lower-case entry) #"^\*?\." "")]
               (or (= "*" entry)
                   (= host domain)
                   (string/ends-with? host (str "." domain)))))
           (remove string/blank? (some-> value (string/split #"\s*,\s*")))))))

(defn proxy-for
  "Returns {:host :port} for the proxy to reach `url` through, or nil to
  connect directly. Follows curl: <scheme>_proxy (e.g. HTTPS_PROXY), then
  ALL_PROXY, skipping hosts in NO_PROXY; the port defaults to 1080.
  Java's HTTP client ignores these variables, so we read them from `env`."
  [url env]
  (let [uri (java.net.URI. url)
        scheme (.getScheme uri)
        [var-name value] (env-entry env [(str scheme "_proxy")
                                         (str (string/upper-case scheme) "_PROXY")
                                         "all_proxy" "ALL_PROXY"])]
    (when (and value (not (no-proxy? (.getHost uri) env)))
      (let [with-scheme (if (string/includes? value "://") value (str "http://" value))
            ^java.net.URI proxy-uri (try (java.net.URI. with-scheme)
                                         (catch java.net.URISyntaxException _ nil))
            host (some-> proxy-uri .getHost)]
        ;; Name the variable but not its value, which may hold credentials
        (when-not host
          (throw (ex-info (str var-name " isn't a valid proxy URL. Expected something like http://proxy.example.com:8080")
                          {:variable var-name})))
        {:host host
         :port (let [port (.getPort proxy-uri)] (if (neg? port) 1080 port))}))))

(defn- request-opts
  "Options for a GET of `url`: `opts`, plus a client that goes through the
  proxy from the environment when one applies."
  [url opts]
  (if-let [p (proxy-for url (System/getenv))]
    (assoc opts :client (client/client (assoc client/default-client-opts :proxy p)))
    opts))

;;;; Downloading default checks

(defn- connection-error
  "Describes a failed connection to `url` for people. Java's connection
  errors often have no message, so fall back to the exception's name."
  [url ^Exception e]
  (let [p (proxy-for url (System/getenv))]
    (str "Couldn't download from " (.getHost (java.net.URI. url))
         (when p (str " through the proxy at " (:host p) ":" (:port p)))
         " (" (or (.getMessage e) (.getSimpleName (class e))) ")."
         " Check your connection and try again.")))

(defn ^:private get-remote-zip!
  "Retrieves default checks, or times out after 5 seconds.

  Note: :timeout applies to the entire HTTP request (connect + read).

  Returns Result<byte-array> - Success with ZIP bytes, or Failure on error."
  [address]
  (result/try-result-with-context
   (fn []
     (let [resp (try
                  (client/get address (request-opts address {:as :bytes
                                                             :timeout 5000
                                                             :throw false}))
                  (catch java.io.IOException e
                    (throw (ex-info (connection-error address e) {:address address} e))))]
       (if (= 200 (:status resp))
         (:body resp)
         (throw (ex-info (str "Couldn't download default checks: " address
                              " returned HTTP " (:status resp) ".")
                        {:status (:status resp)
                         :address address})))))
   {:operation :get-remote-zip :address address}))

(defn ^:private copy!
  "Copies `stream` to `out-file`, creating parent-dir if necessary.
  Used in unzip-file!"
  [stream save-path out-file]
  (file-utils/ensure-parent-dir save-path)
  (io/copy stream out-file))

(defn ^:private unzip-file!
  "Uncompress zip archive, renaming entries from old-name to new-name.

   Options map keys:
   - :input - Zip archive content (byte array or input stream source)
   - :output - Target directory path
   - :old-name - String to replace in entry names
   - :new-name - Replacement string for entry names"
  [{:keys [input output old-name new-name]}]
  (with-open [stream (-> input io/input-stream java.util.zip.ZipInputStream.)]
    (loop [entry (.getNextEntry stream)]
      (when entry
        (let [entry-name (string/replace (.getName entry) old-name new-name)
              save-path (file-utils/join-path output entry-name)
              out-file (io/file save-path)]
          ;; Entries like ../../.bashrc must not land outside output
          (when-not (.startsWith (.normalize (.toAbsolutePath (.toPath out-file)))
                                 (.toAbsolutePath (.toPath (io/file output))))
            (throw (ex-info (str "Refusing to extract " (.getName entry)
                                 ": it points outside " output)
                            {:entry (.getName entry)})))
          (if (.isDirectory entry)
            (file-utils/mkdirs-if-missing save-path)
            (copy! stream save-path out-file))
          (recur (.getNextEntry stream)))))))

(defn ^:private get-remote-version
  "Get the latest commit SHA from GitHub API.
   Returns nil if unable to fetch (offline, rate limit, etc)."
  []
  (try
    (let [resp (client/get remote-api (request-opts remote-api {:timeout 3000
                                                                :throw false}))]
      (when (= 200 (:status resp))
        (let [body (json/parse-string (:body resp) true)]
          (:sha body))))
    (catch Exception _ nil)))

(defn ^:private write-local-version!
  "Write the current version SHA to local version file atomically."
  [sha]
  (when sha
    (let [version-file (sys/filepath ".proserunner" ".version")]
      (file-utils/atomic-spit version-file sha))))

(defn ^:private backup-directory!
  "Create a timestamped backup of a directory."
  [dir-path]
  (let [timestamp (.format (java.text.SimpleDateFormat. "yyyyMMdd-HHmmss")
                           (java.util.Date.))
        backup-dir (file-utils/join-path (sys/home-dir) (str ".proserunner-backup-" timestamp))]
    (when (.exists (io/file dir-path))
      (file-utils/mkdirs-if-missing backup-dir)
      (doseq [^java.io.File file (file-seq (io/file dir-path))]
        (when (.isFile file)
          (let [rel-path (subs (.getPath file) (count dir-path))
                target (io/file (str backup-dir rel-path))]
            (file-utils/ensure-parent-dir (.getPath target))
            (io/copy file target))))
      (console/status "Created backup at: " backup-dir)
      backup-dir)))

(defn ensure-global-config!
  "Ensures ~/.proserunner/ directory and ignore.edn exist.
   Does NOT create config.edn - that should be created by --restore-defaults.
   Idempotent - safe to call multiple times."
  []
  (let [proserunner-dir (sys/filepath ".proserunner")
        ignore-path (sys/filepath ".proserunner" "ignore.edn")]
    (file-utils/mkdirs-if-missing proserunner-dir)
    (when-not (.exists (io/file ignore-path))
      (file-utils/atomic-spit ignore-path default-ignore-template))))

(defn- create-default-config-entry!
  "Creates config.edn with auto-discovered default check entry.
   Discovers .edn files in ~/.proserunner/default/ directory."
  []
  (let [config-path (sys/filepath ".proserunner" "config.edn")
        default-dir (sys/filepath ".proserunner" "default")
        edn-files (check-resolver/get-edn-files default-dir)
        config-content {:checks [{:name "default"
                                  :directory "default"
                                  :files (or edn-files [])}]}]
    (file-utils/atomic-spit config-path
      (str ";; Proserunner Global Configuration\n"
           ";; Auto-generated by --restore-defaults\n\n"
           (with-out-str (pprint/pprint config-content))))))

;;;; Installing default checks

(def ^:private tmp-prefix
  "Prefix for temporary directories inside ~/.proserunner."
  ".tmp-")

(defn- move!
  "Renames `from` to `to` in one atomic step, replacing `to` if it's a file.
  Both must be on the same filesystem."
  [from to]
  (java.nio.file.Files/move
   (.toPath (io/file from))
   (.toPath (io/file to))
   (into-array java.nio.file.StandardCopyOption
               [java.nio.file.StandardCopyOption/ATOMIC_MOVE
                java.nio.file.StandardCopyOption/REPLACE_EXISTING])))

(defn- add-missing!
  "Moves each file under directory `from` to the same place under `to`,
  unless a file is already there."
  [from to]
  (let [base (.toPath (io/file from))]
    (doseq [^java.io.File f (file-seq (io/file from))
            :when (.isFile f)
            :let [target (io/file to (str (.relativize base (.toPath f))))]
            :when (not (.exists target))]
      (file-utils/ensure-parent-dir (.getPath target))
      (move! f target))))

(defn- swap-in!
  "Replaces directory `target` with `replacement` using two renames, so it's
  never half old, half new. Between the renames `target` is briefly missing,
  which the next run treats as missing checks and downloads again."
  [replacement target dir]
  (let [old (io/file dir (str tmp-prefix "replaced-" (System/nanoTime)))]
    (when (.exists (io/file target))
      (move! target old))
    (move! replacement target)
    (file-utils/delete-tree! old)))

(defn- install-staged!
  "Moves checks extracted to `staging` into `dir`:
  - default/ is swapped in whole.
  - config.edn and ignore.edn are skipped; proserunner writes its own when
    they're missing, and never replaces the user's.
  - Directories people add to, like custom/, only gain files they lack.
  - Everything else (README.md, LICENSE.txt) is replaced."
  [staging dir]
  (doseq [^java.io.File f (.listFiles (io/file staging))
          :let [fname (.getName f)
                target (io/file dir fname)]]
    (cond
      (= "default" fname) (swap-in! f target dir)
      (#{"config.edn" "ignore.edn"} fname) nil
      (.isDirectory f) (add-missing! f target)
      :else (move! f target))))

(defn- remove-leftovers!
  "Deletes temporary directories that an interrupted download left in `dir`."
  [dir]
  (doseq [^java.io.File f (.listFiles (io/file dir))
          :when (and (.isDirectory f)
                     (string/starts-with? (.getName f) tmp-prefix))]
    (file-utils/delete-tree! f)))

(defn- install-default-checks!
  "Downloads the default checks into ~/.proserunner.

  The archive is extracted to a temporary directory and moved into place
  from there, so a failed or interrupted download leaves the current
  checks, config, and ignores as they were. The next download deletes
  anything an interrupted one left behind. With `backup?`, the current
  default checks are copied to a timestamped backup first.

  Returns Result with {:created-config? bool}, or Failure on error."
  [{:keys [backup?]}]
  (result/try-result-with-context
   (fn []
     (let [dir (sys/filepath ".proserunner")
           default-dir (sys/filepath ".proserunner" "default")
           config-file (sys/filepath ".proserunner" "config.edn")]
       (file-utils/mkdirs-if-missing dir)
       (remove-leftovers! dir)
       (console/status "Downloading default checks from " remote-address)
       (let [zip-result (get-remote-zip! remote-address)
             _ (when (result/failure? zip-result)
                 (throw (ex-info (:error zip-result) (:context zip-result))))
             staging (str (java.nio.file.Files/createTempDirectory
                           (.toPath (io/file dir))
                           tmp-prefix
                           (make-array java.nio.file.attribute.FileAttribute 0)))]
         (try
           (unzip-file! {:input (:value zip-result)
                         :output staging
                         :old-name "proserunner-default-checks-main/"
                         :new-name ""})
           (when (and backup? (.exists (io/file default-dir)))
             (console/status "Backing up existing checks...")
             (backup-directory! default-dir))
           (install-staged! staging dir)
           (finally
             (file-utils/delete-tree! staging))))
       (when-let [version (get-remote-version)]
         (write-local-version! version))
       (let [create? (not (.exists (io/file config-file)))]
         (when create?
           (create-default-config-entry!))
         {:created-config? create?})))
   {:operation :download-checks}))

(defn restore-defaults!
  "Downloads fresh default checks, backing up the current ones first.
  Keeps config.edn, ignore.edn, and custom checks.

  Returns Result<nil> - Success when complete, Failure on error."
  []
  (console/status "Restoring default checks...")
  (result/bind
   (install-default-checks! {:backup? true})
   (fn [{:keys [created-config?]}]
     (when created-config?
       (console/status "Created config.edn with default check entry"))
     (console/status "Default checks restored. Your config, ignores, and custom checks were kept.")
     (result/ok nil))))

(defn initialize-proserunner
  "Sets up ~/.proserunner directory and downloads default checks for first-time users.

   Returns Result with config on success, or Failure with error details."
  [default-config]
  (console/status "First run: setting up ~/.proserunner")
  (let [dl-result (install-default-checks! {:backup? false})]
    (if (result/failure? dl-result)
      (result/map-err dl-result #(str "Failed to download default checks: " %))
      (do
        (console/status "Created Proserunner directory: " (sys/filepath ".proserunner/"))
        (console/status "You can store custom checks in: " (sys/filepath ".proserunner" "custom/"))
        (console/status "To update the default checks later, run: proserunner --restore-defaults")
        (loader/load-config-from-file default-config)))))

(defn default
  "If current config isn't valid, use the default."
  [options]
  (let [cur-config (:config options)
        new-config (sys/filepath ".proserunner" "config.edn")]
    (if (or (nil? cur-config)
            (not (.exists (io/file cur-config))))
      (assoc options :config new-config)
      options)))

(defn- ensure-checks-exist!
  "Ensures that checks directories referenced by config exist.
   Downloads default checks if missing and referenced.

   Returns Result<nil> on success, or Failure with error details."
  [check-entries global-checks]
  (let [missing (check-resolver/missing-global-check-directories check-entries global-checks)]
    (if (seq missing)
      (if (contains? missing "default")
        (do
          (console/status "Default checks not found.")
          (let [default-config (sys/filepath ".proserunner" "config.edn")
                init-result (initialize-proserunner default-config)]
            (if (result/success? init-result)
              (result/ok nil)
              init-result)))
        ;; For now, only handle "default". "custom" would need similar logic
        (result/ok nil))
      (result/ok nil))))

(defn- load-project-based-config
  "Loads project configuration, merging with global config as appropriate.
   Ensures referenced check directories exist, downloading if necessary.

   Returns Result with Config record on success, or Failure with error details."
  [current-dir]
  ;; First, find and read the manifest to get raw check entries
  (if-let [{:keys [manifest-path]} (manifest/find current-dir)]
    (let [project-manifest (manifest/read manifest-path)
          global-cfg (project-config/load-global-config)
          ;; Merge configs to get final check entries list
          merged (merger/merge-configs global-cfg project-manifest)
          check-entries (:checks merged)
          global-checks (or (:checks global-cfg) [])
          ;; Ensure checks exist before proceeding
          ensure-result (ensure-checks-exist! check-entries global-checks)]
      (if (result/success? ensure-result)
        ;; Now load the full config (which will resolve checks)
        (let [project-cfg (project-config/load current-dir)]
          (result/ok (map->Config {:checks (:checks project-cfg)
                                   :ignore (:ignore project-cfg)})))
        ensure-result))
    (result/err "Failed to load project configuration. Check .proserunner/config.edn for errors."
                {:current-dir current-dir})))

(defn determine-config-strategy
  "Determines config loading strategy based on inputs.

  Returns strategy keyword: :custom | :project | :global | :initialize

  Context map keys:
  - :using-default? - Is the default config path being used?
  - :custom-exists? - Does the custom config file exist?
  - :in-project? - Is the current directory in a project?
  - :config-exists? - Does the global config file exist?

  Default checks are never updated implicitly; --restore-defaults does that."
  [{:keys [using-default? custom-exists? in-project? config-exists?]}]
  (cond
    ;; Custom config file specified via -c
    (and (not using-default?) custom-exists?)
    :custom

    ;; In project directory
    (and using-default? in-project?)
    :project

    ;; Global config exists
    config-exists?
    :global

    ;; First-time initialization
    :else
    :initialize))

(defn fetch-or-create!
  "Fetches or creates config file. Will exit on failure.
   Downloads default checks on first run only; it doesn't check for
   updates, so results don't change between runs unless the user runs
   --restore-defaults.

   If in a project directory, loads project config which may include
   project-specific checks and ignores merged with global config."
  [config-filepath]
  (when (and config-filepath (not (string? config-filepath)))
    (throw (ex-info (str "fetch-or-create! expects a string filepath, got: " (type config-filepath))
                    {:config-filepath config-filepath})))
  (let [default-config (sys/filepath ".proserunner" "config.edn")
        using-default? (or (nil? config-filepath) (= config-filepath default-config))
        checks-dir-exists? (.exists (io/file (sys/filepath ".proserunner" "default")))
        current-dir (System/getProperty "user.dir")
        in-project? (manifest/find current-dir)]

    ;; Ensure checks are initialized before doing anything else
    (when (and using-default? (not checks-dir-exists?))
      (result/result-or-exit (initialize-proserunner default-config)))

    ;; Determine strategy and execute
    (let [config-exists? (.exists (io/file default-config))
          custom-exists? (and config-filepath (.exists (io/file config-filepath)))
          strategy (determine-config-strategy
                     {:using-default? using-default?
                      :custom-exists? custom-exists?
                      :in-project? in-project?
                      :config-exists? config-exists?})]
      (case strategy
        :custom (result/result-or-exit (loader/load-config-from-file config-filepath))
        :project (result/result-or-exit (load-project-based-config current-dir))
        :global (result/result-or-exit (loader/load-config-from-file default-config))
        :initialize (result/result-or-exit (loader/load-config-from-file default-config))))))
