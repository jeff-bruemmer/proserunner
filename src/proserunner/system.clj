(ns proserunner.system
  "Utilites for proserunner to figure out what OS it's on so it can follow system mores."
  (:gen-class)
  (:require [proserunner.console :as console]
            [proserunner.file-utils :as file-utils]
            [clojure.string :as string]))

(set! *warn-on-reflection* true)

(defn home-dir
  "The user's home directory."
  []
  (System/getProperty "user.home"))

(defn xdg-config-home
  "$XDG_CONFIG_HOME, or nil when it's unset or relative (the XDG spec says
  to ignore relative paths). A function so tests can replace it."
  []
  (let [v (System/getenv "XDG_CONFIG_HOME")]
    (when (and v (.isAbsolute (java.io.File. ^String v)))
      v)))

(defn legacy-config-dir
  "Where releases before XDG support kept the global config: ~/.proserunner."
  []
  (file-utils/join-path (home-dir) ".proserunner"))

(defn xdg-config-dir
  "$XDG_CONFIG_HOME/proserunner, defaulting to ~/.config/proserunner."
  []
  (file-utils/join-path (or (xdg-config-home)
                            (file-utils/join-path (home-dir) ".config"))
                        "proserunner"))

(defn config-dir
  "The global config directory: the XDG one, unless only ~/.proserunner
  exists, which happens when moving it there failed (see
  proserunner.config/migrate-legacy-config!)."
  []
  (let [^String xdg (xdg-config-dir)
        ^String legacy (legacy-config-dir)]
    (if (and (not (.exists (java.io.File. xdg)))
             (.isDirectory (java.io.File. legacy)))
      legacy
      xdg)))

(defn config-path
  "Joins `parts` onto the global config directory."
  [& parts]
  (apply file-utils/join-path (config-dir) parts))

(defn display-path
  "Shortens a path under the home directory to start with ~."
  [^String path]
  (let [^String home (home-dir)]
    (if (and home (string/starts-with? path home))
      (str "~" (subs path (count home)))
      path)))

(defn call-with-config-lock
  "Calls f while holding the lock that serializes proserunner's
  read-modify-write updates (ignore lists, config.edn, default checks)
  across processes. The lock file lives in the global config directory."
  [f]
  (file-utils/call-with-lock (config-path ".lock") f))

(defn check-dir
  "Infer the directory when supplied a config filepath.
  The config file must be in the same directory as the check directories."
  [config]
  (let [dd (str (config-dir) java.io.File/separator)]
    (if (nil? config)
      (do (console/status "Using default directory: " dd)
          dd)
      (-> config
          (string/split (re-pattern java.io.File/separator))
          drop-last
          (#(string/join "/" %))
          (str java.io.File/separator)))))
