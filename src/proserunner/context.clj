(ns proserunner.context
  "Unified context resolution utilities for project vs global scope operations.
   Provides a single source of truth for determining operation context."
  (:require [proserunner.project-config :as project-config]
            [proserunner.config.manifest :as manifest])
  (:gen-class))

(set! *warn-on-reflection* true)

(defn determine-context
  "Unified context determination with configurable features.

   Provides single source of truth for all scope resolution needs across
   commands, effects, and file operations.

   Options:
   - :global - Force global scope (the global config directory)
   - :project - Force project scope (.proserunner/)
   - :start-dir - Starting directory for project detection
   - :include-project-root - Include :project-root in result (default true if :project)
   - :include-alt-msg - Include alternate scope suggestion message
   - :alt-msg-template - Custom alternate message template
   - :normalize-opts - Return :opts-with-target for command handlers

   Returns map with:
   - :target - :global or :project keyword
   - :msg-context - \"global\" or \"project\" string for display
   - :start-dir - Resolved start directory (if applicable)
   - :project-root - Project root path (if :target is :project and requested)
   - :alt-msg - Alternate scope message (if :include-alt-msg is true)
   - :opts-with-target - Normalized opts map (if :normalize-opts is true)"
  [options]
  (let [start-dir (project-config/resolve-start-dir options)
        target (project-config/determine-target options start-dir)
        base-context {:target target
                      :msg-context (if (= target :project) "project" "global")
                      :start-dir start-dir}
        ;; Add project-root if in project context and requested (default true)
        with-project-root (if (and (= target :project)
                                   (get options :include-project-root true))
                            (assoc base-context :project-root
                                   (:project-root (manifest/find start-dir)))
                            base-context)
        ;; Add alternate message if requested
        with-alt-msg (if (:include-alt-msg options)
                       (assoc with-project-root :alt-msg
                              (or (:alt-msg-template options)
                                  (if (= target :project)
                                    "Use --global to add to global ignore list instead."
                                    "Use --project to add to project ignore list instead.")))
                       with-project-root)
        ;; Add normalized opts if requested (for command handlers)
        with-opts (if (:normalize-opts options)
                    (assoc with-alt-msg :opts-with-target
                           (assoc options :project (= target :project)))
                    with-alt-msg)]
    with-opts))

(defn with-context
  "Determines operation context and executes function with context map.

   Provides unified context resolution for operations that can target
   either global (the global config directory) or project (.proserunner/) scope.

   The function f receives a map with:
   - :target - either :global or :project
   - :start-dir - resolved start directory
   - :project-root - project root directory (only when :target is :project)

   Options:
   - :global - Force global scope (the global config directory)
   - :project - Force project scope (.proserunner/)
   - :start-dir - Starting directory for project detection

   Example:
     (with-context {:project true}
       (fn [{:keys [target project-root]}]
         (if (= target :project)
           (do-project-operation project-root)
           (do-global-operation))))"
  [options f]
  (f (determine-context options)))
