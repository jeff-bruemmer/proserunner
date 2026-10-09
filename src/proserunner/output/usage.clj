(ns proserunner.output.usage
  "Help and usage messages."
  (:refer-clojure :exclude [print])
  (:gen-class)
  (:require [clojure.string :as string]
            [proserunner.console :as console]
            [proserunner.system :as sys]
            [proserunner.version :as ver]))

(set! *warn-on-reflection* true)

(def docs-url "https://github.com/jeff-bruemmer/proserunner#readme")

(defn version
  "Prints version number."
  []
  (println "Proserunner version:" ver/number))

(defn- long-opt
  "The long option of a summary entry, e.g. \"--file\"."
  [opt]
  (or (:long-opt opt)
      (last (string/split (string/trim (:option opt)) #", "))))

(defn categorize-options
  "Organizes command-line options into logical categories for better help output."
  [summary]
  (let [category-order ["Checking"
                        "Ignoring issues from a run"
                        "Command options"
                        "Other options"]
        categories {"Checking" #{"--output" "--quiet" "--exclude" "--code-blocks"
                                 "--quoted-text" "--skip-ignore" "--file" "--config"}
                    "Ignoring issues from a run" #{"--ignore-issues" "--ignore-all"}
                    "Command options" #{"--global" "--project" "--force" "--name"}}
        categorized (reduce (fn [acc opt]
                              (let [category (or (some (fn [[cat opts]] (when (opts (long-opt opt)) cat))
                                                       categories)
                                                 "Other options")]
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

(defn- print-options-named
  "Prints the summary entries for the long options in `names`, in that order."
  [summary names]
  (let [by-name (into {} (map (juxt long-opt identity)) summary)]
    (doseq [opt (keep by-name names)]
      (print-option-with-desc opt))))

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
  (println "  Check text from another program:")
  (println "    pandoc -t markdown report.docx | proserunner -")
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
  (println "  Ignore a word everywhere, or find stale ignores:")
  (println "    proserunner ignore add hopefully")
  (println "    proserunner ignore audit"))

(defn- print-main
  "Prints full usage."
  [{:keys [summary config]}]
  (println "\nP R O S E R U N N E R\n")
  (println "Fast prose linter. Finds issues, lets you ignore what you don't care about.\n")
  (println "USAGE:")
  (println "  proserunner [OPTIONS] PATH...   Check files and directories; - reads standard input")
  (println "  proserunner COMMAND [ARGS]\n")
  (println "COMMANDS:")
  (println "  check PATH...    Check files (the default, so 'check' is optional)")
  (println "  ignore           Ignore words everywhere, list ignores, clean up stale ones")
  (println "  checks           List checks, import your own, or restore the defaults")
  (println "  init             Set up .proserunner/ for a project in this directory")
  (println "  help [COMMAND]   Show help for a command\n")
  (print-examples)
  (print-categorized-options summary)
  (println "\nEXIT STATUS:")
  (println "  0 no issues, 1 issues found, 2 error")
  (println "\nCONFIG:")
  (println (str "  Global: " (sys/display-path (sys/config-path "config.edn"))))
  (println "  Project: .proserunner/config.edn")
  (println (str "  Current: " (some-> config sys/display-path)))
  (println "\nDocs:   " docs-url)
  (println "Issues: " console/issues-url)
  (println)
  (version))

(def ^:private scope-note
  ["Inside a project (a directory with .proserunner/config.edn), this uses"
   "the project's config; elsewhere, the global one. --global or --project"
   "chooses."])

(defn- print-ignore
  [{:keys [summary]}]
  (println "proserunner ignore: ignore words everywhere, and keep ignore lists tidy.\n")
  (println "USAGE:")
  (println "  proserunner ignore add SPECIMEN      Ignore a word or phrase everywhere")
  (println "  proserunner ignore remove SPECIMEN   Stop ignoring it (alias: rm)")
  (println "  proserunner ignore list              List ignores (alias: ls)")
  (println "  proserunner ignore clear             Remove every ignore; asks first in a terminal")
  (println "  proserunner ignore audit             List ignores for files that no longer exist")
  (println "  proserunner ignore clean             Remove the ignores audit lists\n")
  (println "To ignore issues from a run, add --ignore-issues or --ignore-all to it:")
  (println "  proserunner doc.md --ignore-issues 1,3\n")
  (println "OPTIONS:")
  (print-options-named summary ["--global" "--project" "--force"])
  (println)
  (run! println scope-note))

(defn- print-checks
  [{:keys [summary]}]
  (println "proserunner checks: list checks, import your own, or restore the defaults.\n")
  (println "USAGE:")
  (println "  proserunner checks           List enabled checks (also: checks list)")
  (println "  proserunner checks add DIR   Import the .edn checks in DIR")
  (println "  proserunner checks restore   Reinstall the default checks this version ships with,")
  (println "                               keeping your config, ignores, and custom checks\n")
  (println "OPTIONS:")
  (print-options-named summary ["--name" "--global" "--project" "--config"])
  (println)
  (run! println scope-note))

(defn- print-init
  [_]
  (println "proserunner init: set up a project in the current directory.\n")
  (println "USAGE:")
  (println "  proserunner init\n")
  (println "Creates .proserunner/config.edn and .proserunner/checks/. Commit them so")
  (println "everyone on the project gets the same checks and ignores."))

(defn print
  "Prints help for (:help-topic opts), or full usage."
  [opts]
  (case (:help-topic opts)
    "ignore" (print-ignore opts)
    "checks" (print-checks opts)
    "init" (print-init opts)
    (print-main opts)))

(defn print-concise
  "Prints a short usage message for when no path or action is given."
  []
  (println "P R O S E R U N N E R\n")
  (println "A fast, customizable prose linter.\n")
  (println "Usage: proserunner [OPTIONS] PATH...")
  (println "       proserunner COMMAND [ARGS]\n")
  (println "Examples:")
  (println "  proserunner document.md                Check a file")
  (println "  proserunner docs/ -o plain             Check a directory, one issue per line")
  (println "  proserunner doc.md --ignore-issues 1,3 Ignore issues by number")
  (println "  proserunner ignore add hopefully       Ignore a word everywhere\n")
  (println "Commands: check, ignore, checks, init, help")
  (println "Run 'proserunner --help' for all options."))
