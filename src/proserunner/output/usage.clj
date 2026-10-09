(ns proserunner.output.usage
  "Help and usage messages."
  (:refer-clojure :exclude [print])
  (:gen-class)
  (:require [clojure.string :as string]
            [proserunner.version :as ver]))

(set! *warn-on-reflection* true)

(def docs-url "https://github.com/jeff-bruemmer/proserunner#readme")
(def issues-url "https://github.com/jeff-bruemmer/proserunner/issues")

(defn version
  "Prints version number."
  []
  (println "Proserunner version:" ver/number))

(defn categorize-options
  "Organizes command-line options into logical categories for better help output."
  [summary]
  (let [category-order ["Basic Usage"
                        "Output Options"
                        "Ignore Management"
                        "Ignore Maintenance"
                        "Configuration & Checks"
                        "Scope Control"
                        "Advanced Options"]
        categories {"Basic Usage" #{"--file" "--help" "--version"}
                    "Output Options" #{"--output" "--quiet" "--code-blocks" "--quoted-text"}
                    "Ignore Management" #{"--add-ignore" "--remove-ignore" "--list-ignored"
                                         "--clear-ignored" "--force" "--ignore-all" "--ignore-issues"}
                    "Ignore Maintenance" #{"--audit-ignores" "--clean-ignores"}
                    "Configuration & Checks" #{"--init-project" "--restore-defaults"
                                               "--add-checks" "--checks" "--exclude"}
                    "Scope Control" #{"--global" "--project"}
                    "Advanced Options" #{"--config" "--ignore" "--no-cache" "--skip-ignore"
                                        "--parallel-files" "--sequential-lines" "--timer" "--name"}}
        ;; Group options by category
        categorized (reduce (fn [acc opt]
                             (let [long-opt (or (:long-opt opt)
                                                (last (string/split (string/trim (:option opt)) #", ")))
                                   category (some (fn [[cat opts]] (when (opts long-opt) cat)) categories)
                                   category (or category "Advanced Options")]
                               (update acc category (fnil conj []) opt)))
                           {}
                           summary)]
    ;; Return categories in the specified order
    (map (fn [cat] [cat (get categorized cat)])
         (filter #(get categorized %) category-order))))

(defn- print-option-with-desc
  "Prints a single option with description on separate line if long."
  [{:keys [option required desc]}]
  (let [opt-line (str "  " option (when required (str " " required)))]
    (println opt-line)
    (println (str "      " desc))))

(defn- print-categorized-options
  "Prints options organized by category with descriptions on separate lines."
  [summary]
  (let [categories (categorize-options summary)]
    (doseq [[category options] categories]
      (println (str "\n" category ":"))
      (doseq [opt options]
        (print-option-with-desc opt)))))

(defn- print-examples
  "Prints usage examples, simplest first."
  []
  (println "EXAMPLES:")
  (println "  Check a file:")
  (println "    proserunner document.md")
  (println)
  (println "  Check every markdown, text, tex, and org file under a directory:")
  (println "    proserunner docs/")
  (println)
  (println "  Ignore issues by the numbers shown in the last run:")
  (println "    proserunner README.md --ignore-issues 1,3,5-7")
  (println)
  (println "  One issue per line, for grep or your editor:")
  (println "    proserunner docs/ -o plain")
  (println)
  (println "  Exclude paths:")
  (println "    proserunner . --exclude \"drafts/**,*.log\"")
  (println)
  (println "  List enabled checks, or find stale ignores:")
  (println "    proserunner --checks")
  (println "    proserunner --audit-ignores"))

(defn print
  "Prints full usage."
  [{:keys [summary config]}]
  (println "\nP R O S E R U N N E R\n")
  (println "Fast prose linter. Finds issues, lets you ignore what you don't care about.\n")
  (println "USAGE:")
  (println "  proserunner [OPTIONS] [PATH...]\n")
  (print-examples)
  (print-categorized-options summary)
  (println "\nEXIT STATUS:")
  (println "  0 no issues, 1 issues found, 2 error")
  (println "\nCONFIG:")
  (println "  Global: ~/.proserunner/config.edn")
  (println "  Project: .proserunner/config.edn")
  (println (str "  Current: " config))
  (println "\nDocs:   " docs-url)
  (println "Issues: " issues-url)
  (println)
  (version))

(defn print-concise
  "Prints a short usage message for when no path or action is given."
  []
  (println "P R O S E R U N N E R\n")
  (println "A fast, customizable prose linter.\n")
  (println "Usage: proserunner [OPTIONS] [PATH...]\n")
  (println "Examples:")
  (println "  proserunner document.md                Check a file")
  (println "  proserunner docs/ -o plain             Check a directory, one issue per line")
  (println "  proserunner doc.md --ignore-issues 1,3 Ignore issues by number\n")
  (println "Run 'proserunner --help' for all options."))
