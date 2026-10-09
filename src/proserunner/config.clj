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
            [clojure.pprint :as pprint]))

(set! *warn-on-reflection* true)

;; Each release installs one fixed commit of the default checks, so a
;; fresh install or `checks restore` never picks up checks the release
;; wasn't tested with. Upgrading doesn't reinstall existing checks; .version
;; records the installed ref so a later release can notice the mismatch.
;; To ship new checks, run `bb pin-checks` and paste its output here.
(def default-checks-ref "d4597fbf0f41f93b46511a2706c4e56c7bf6292b")
(def default-checks-sha256 "53dcc123317e0341e61ec3ed9f1b018329d81818cde8620316a41e95b994871a")

(def remote-address
  (str "https://github.com/jeff-bruemmer/proserunner-default-checks/archive/"
       default-checks-ref ".zip"))

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

(defn- archive-path
  "An archive entry's path without the top-level directory GitHub puts
  everything under, or nil for that directory itself."
  [entry-name]
  (let [[_ path] (re-find #"^[^/]*/(.*)$" entry-name)]
    (when-not (string/blank? path)
      path)))

(defn archive-digest
  "SHA-256 of the files in a default-checks archive: each file's path (see
  archive-path), size, and bytes, in path order. Hashing the contents
  rather than the zip keeps the digest stable if GitHub changes how it
  builds archives."
  [zip-bytes]
  (let [files (with-open [zis (java.util.zip.ZipInputStream.
                               (java.io.ByteArrayInputStream. zip-bytes))]
                (loop [files (sorted-map)]
                  (if-let [entry (.getNextEntry zis)]
                    (let [path (archive-path (.getName entry))]
                      (recur (if (and path (not (.isDirectory entry)))
                               (assoc files path (.readAllBytes zis))
                               files)))
                    files)))
        md (java.security.MessageDigest/getInstance "SHA-256")]
    (doseq [[^String path ^bytes content] files]
      (.update md (.getBytes (str path "\n" (alength content) "\n") "UTF-8"))
      (.update md content))
    (format "%064x" (BigInteger. 1 (.digest md)))))

(defn- verify-archive!
  "Throws unless `zip-bytes` holds exactly the pinned default checks."
  [zip-bytes]
  (let [actual (archive-digest zip-bytes)]
    (when-not (= actual default-checks-sha256)
      (throw (ex-info (str "The default checks downloaded from " remote-address
                           " don't match the checksum this version of proserunner expects,"
                           " so nothing was installed. Try again; if it keeps happening, please report it: "
                           console/issues-url)
                      {:expected default-checks-sha256 :actual actual})))))

(defn ^:private unzip-file!
  "Extracts a zip archive into `output`, dropping the top-level directory
  (see archive-path)."
  [input output]
  (with-open [stream (-> input io/input-stream java.util.zip.ZipInputStream.)]
    (loop [entry (.getNextEntry stream)]
      (when entry
        (when-let [entry-name (archive-path (.getName entry))]
          (let [save-path (file-utils/join-path output entry-name)
                out-file (io/file save-path)]
            ;; Entries like ../../.bashrc must not land outside output
            (when-not (.startsWith (.normalize (.toAbsolutePath (.toPath out-file)))
                                   (.toAbsolutePath (.toPath (io/file output))))
              (throw (ex-info (str "Refusing to extract " (.getName entry)
                                   ": it points outside " output)
                              {:entry (.getName entry)})))
            (if (.isDirectory entry)
              (file-utils/mkdirs-if-missing save-path)
              (copy! stream save-path out-file))))
        (recur (.getNextEntry stream))))))

(defn ^:private write-local-version!
  "Records which commit of the default checks is installed."
  []
  (file-utils/atomic-spit (sys/config-path ".version") default-checks-ref))

(defn ^:private backup-directory!
  "Copies `dir-path` to a timestamped directory under backups/ in the
  config directory."
  [dir-path]
  (let [timestamp (.format (java.text.SimpleDateFormat. "yyyyMMdd-HHmmss")
                           (java.util.Date.))
        backup-dir (sys/config-path "backups" (str "default-" timestamp))]
    (when (.exists (io/file dir-path))
      (file-utils/mkdirs-if-missing backup-dir)
      (doseq [^java.io.File file (file-seq (io/file dir-path))]
        (when (.isFile file)
          (let [rel-path (subs (.getPath file) (count dir-path))
                target (io/file (str backup-dir rel-path))]
            (file-utils/ensure-parent-dir (.getPath target))
            (io/copy file target))))
      (console/status "Created backup at: " (sys/display-path backup-dir))
      backup-dir)))

(defn- create-default-config-entry!
  "Creates config.edn with auto-discovered default check entry.
   Discovers .edn files in the default/ directory."
  []
  (let [config-path (sys/config-path "config.edn")
        default-dir (sys/config-path "default")
        edn-files (check-resolver/get-edn-files default-dir)
        config-content {:checks [{:name "default"
                                  :directory "default"
                                  :files (or edn-files [])}]}]
    (file-utils/atomic-spit config-path
      (str ";; Proserunner Global Configuration\n"
           ";; Auto-generated when the default checks were installed\n\n"
           (with-out-str (pprint/pprint config-content))))))

;;;; Installing default checks

(def ^:private tmp-prefix
  "Prefix for temporary directories inside the config directory."
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
  "Downloads the pinned default checks into the config directory.

  The archive's checksum is verified, then it's extracted to a temporary
  directory and moved into place from there, so a failed or interrupted
  download leaves the current checks, config, and ignores as they were.
  The next download deletes anything an interrupted one left behind. With
  `backup?`, the current default checks are copied to a timestamped backup
  first. Everything after the download holds the config lock, so two runs
  can't install over each other or delete each other's staging directory.

  Returns Result with {:created-config? bool}, or Failure on error."
  [{:keys [backup?]}]
  (result/try-result-with-context
   (fn []
     (console/status "Downloading default checks from " remote-address)
     (let [zip-result (get-remote-zip! remote-address)
           _ (when (result/failure? zip-result)
               (throw (ex-info (:error zip-result) (:context zip-result))))
           zip (:value zip-result)]
       (verify-archive! zip)
       (sys/call-with-config-lock
        (fn []
          (let [dir (sys/config-dir)
                default-dir (sys/config-path "default")
                config-file (sys/config-path "config.edn")]
            (file-utils/mkdirs-if-missing dir)
            (remove-leftovers! dir)
            (let [staging (str (java.nio.file.Files/createTempDirectory
                                (.toPath (io/file dir))
                                tmp-prefix
                                (make-array java.nio.file.attribute.FileAttribute 0)))]
              (try
                (unzip-file! zip staging)
                (when (and backup? (.exists (io/file default-dir)))
                  (console/status "Backing up existing checks...")
                  (backup-directory! default-dir))
                (install-staged! staging dir)
                (finally
                  (file-utils/delete-tree! staging))))
            (write-local-version!)
            (let [create? (not (.exists (io/file config-file)))]
              (when create?
                (create-default-config-entry!))
              {:created-config? create?}))))))
   {:operation :download-checks}))

(defn restore-defaults!
  "Reinstalls the default checks this version ships with, backing up the
  current ones first. Keeps config.edn, ignore.edn, and custom checks.

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
  "Sets up the config directory and downloads default checks for first-time users.

   Returns Result with config on success, or Failure with error details."
  [default-config]
  (console/status "First run: setting up " (sys/display-path (sys/config-dir)))
  (let [dl-result (install-default-checks! {:backup? false})]
    (if (result/failure? dl-result)
      (result/map-err dl-result #(str "Failed to download default checks: " %))
      (do
        (console/status "You can store custom checks in: "
                        (sys/display-path (sys/config-path "custom/")))
        (loader/load-config-from-file default-config)))))

;;;; The global config directory

(defn migrate-legacy-config!
  "Moves ~/.proserunner, where earlier releases kept the global config, to
  the XDG config directory. If the move fails (for example, across
  filesystems), sys/config-dir keeps using ~/.proserunner, and this warns
  each run with the command to move it by hand."
  []
  (let [legacy (io/file (sys/legacy-config-dir))
        target (io/file (sys/xdg-config-dir))
        shown #(sys/display-path (str %))]
    (when (.isDirectory legacy)
      (if (.exists target)
        (console/warn "Both " (shown legacy) " and " (shown target) " exist; using "
                      (shown target) ". Remove " (shown legacy)
                      " once you've copied anything you need from it.")
        (try
          (file-utils/mkdirs-if-missing (.getParent target))
          (move! legacy target)
          (console/status "Moved " (shown legacy) " to " (shown target)
                          ", the standard place for config files.")
          (catch Exception e
            ;; Another run may have just moved it
            (when-not (.exists target)
              (console/warn "Couldn't move " (shown legacy) " to " (shown target)
                            " (" (or (.getMessage e) (.getSimpleName (class e)))
                            "), so proserunner is still using it. To move it yourself: mv "
                            (shown legacy) " " (shown target)))))))))

(defn ensure-global-config!
  "Moves a legacy ~/.proserunner into place, then ensures the config
  directory and ignore.edn exist. Does NOT create config.edn; installing
  the default checks does that. Idempotent - safe to call multiple times."
  []
  (migrate-legacy-config!)
  (let [ignore-path (sys/config-path "ignore.edn")]
    (file-utils/mkdirs-if-missing (sys/config-dir))
    (when-not (.exists (io/file ignore-path))
      (file-utils/atomic-spit ignore-path default-ignore-template))))

(defn default
  "If current config isn't valid, use the default."
  [options]
  (let [cur-config (:config options)
        new-config (sys/config-path "config.edn")]
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
          (let [default-config (sys/config-path "config.edn")
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

  Default checks are never updated implicitly; `proserunner checks restore` does that."
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
   `proserunner checks restore`.

   If in a project directory, loads project config which may include
   project-specific checks and ignores merged with global config."
  [config-filepath]
  (when (and config-filepath (not (string? config-filepath)))
    (throw (ex-info (str "fetch-or-create! expects a string filepath, got: " (type config-filepath))
                    {:config-filepath config-filepath})))
  (let [default-config (sys/config-path "config.edn")
        using-default? (or (nil? config-filepath) (= config-filepath default-config))
        checks-dir-exists? (.exists (io/file (sys/config-path "default")))
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
