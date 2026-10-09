(ns proserunner.ignore.context
  "Scope resolution for project vs global ignore lists."
  (:refer-clojure :exclude [list])
  (:gen-class)
  (:require [proserunner.context :as context]
            [proserunner.ignore.core :as core]
            [proserunner.ignore.file :as file]
            [proserunner.project-config :as project-config]
            [proserunner.system :as sys]))

(set! *warn-on-reflection* true)

;;; Private helper functions for project scope

(defn- add-ignore-to-project!
  "Adds a specimen to project .proserunner/config.edn.
   Strings go to :ignore, maps go to :ignore-issues."
  [specimen project-root]
  (let [config (project-config/read project-root)]
    (if (string? specimen)
      (let [current-ignore (or (:ignore config) #{})
            updated-ignore (core/add-to-set current-ignore specimen)]
        (project-config/write! project-root (assoc config :ignore updated-ignore)))
      (let [current-issues (or (:ignore-issues config) #{})
            updated-issues (conj current-issues specimen)]
        (project-config/write! project-root (assoc config :ignore-issues updated-issues))))))

(defn- remove-ignore-from-project!
  "Removes a specimen from project .proserunner/config.edn.
   Removes strings from :ignore, maps from :ignore-issues."
  [specimen project-root]
  (let [config (project-config/read project-root)]
    (if (string? specimen)
      (let [current-ignore (or (:ignore config) #{})
            updated-ignore (core/remove-from-set current-ignore specimen)]
        (project-config/write! project-root (assoc config :ignore updated-ignore)))
      (let [current-issues (or (:ignore-issues config) #{})
            updated-issues (disj current-issues specimen)]
        (project-config/write! project-root (assoc config :ignore-issues updated-issues))))))

;;; Context-Aware Public API

(defn add!
  "Adds a specimen to the ignore list with context-aware targeting.
   Strings go to :ignore, maps go to :ignore-issues.

   Options:
   - :global - Force global scope (ignore.edn in the global config directory)
   - :project - Force project scope (.proserunner/config.edn)
   - :start-dir - Starting directory for project detection"
  [specimen options]
  (context/with-context options
    (fn [{:keys [target project-root]}]
      (sys/call-with-config-lock
       #(if (= target :global)
          (file/write! (core/add-specimen (file/read) specimen))
          (add-ignore-to-project! specimen project-root))))))

(defn remove!
  "Removes a specimen from the ignore list with context-aware targeting.
   Removes strings from :ignore, maps from :ignore-issues.

   Options:
   - :global - Force global scope (ignore.edn in the global config directory)
   - :project - Force project scope (.proserunner/config.edn)
   - :start-dir - Starting directory for project detection"
  [specimen options]
  (context/with-context options
    (fn [{:keys [target project-root]}]
      (sys/call-with-config-lock
       #(if (= target :global)
          (file/write! (core/remove-specimen (file/read) specimen))
          (remove-ignore-from-project! specimen project-root))))))

(defn list
  "Returns map with :ignore (set) and :ignore-issues (vector) with context-aware targeting.

   When in project context with :extend mode, shows both global and project ignores.
   When in project context with :replace mode, shows only project ignores.
   When in global context, shows only global ignores.

   Options:
   - :global - Force global scope
   - :project - Force project scope
   - :start-dir - Starting directory for project detection"
  ([]
   (file/read))
  ([options]
   (context/with-context options
     (fn [{:keys [target start-dir]}]
       (if (= target :global)
         (file/read)
         ;; List from project context (respecting merge mode)
         (let [config (project-config/load start-dir)]
           ;; The project-config loader already merges global and project ignores
           ;; based on :ignore-mode
           {:ignore (:ignore config)
            :ignore-issues (:ignore-issues config)}))))))

(defn clear!
  "Clears all ignored specimens and issues with context-aware targeting.
   Takes the same options as add!."
  [options]
  (context/with-context options
    (fn [{:keys [target project-root]}]
      (sys/call-with-config-lock
       #(if (= target :global)
          (file/write! {:ignore #{} :ignore-issues #{}})
          ;; Keep the rest of the project config
          (let [config (project-config/read project-root)]
            (project-config/write! project-root (assoc config
                                                       :ignore #{}
                                                       :ignore-issues #{}))))))))
