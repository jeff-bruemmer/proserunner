(ns proserunner.console
  "Messages for people: status, warnings, and errors.

  Everything here goes to stderr, so stdout carries only a command's
  primary output (lint results, check lists, help) and stays safe to pipe.
  Each message is written with a single write call so messages from
  parallel workers don't interleave mid-line."
  (:gen-class)
  (:require [clojure.string :as string]))

(set! *warn-on-reflection* true)

(def issues-url "https://github.com/jeff-bruemmer/proserunner/issues")

(def ^:dynamic *quiet*
  "When true, status messages are suppressed. Warnings and errors still print."
  false)

(defn debug?
  "True when PROSERUNNER_DEBUG is set to a non-empty value."
  []
  (not (string/blank? (System/getenv "PROSERUNNER_DEBUG"))))

(defn- emit!
  [& parts]
  (let [^java.io.Writer w *err*]
    (.write w (str (apply str parts) "\n"))
    (.flush w)))

(defn status
  "Prints a progress or confirmation message. Suppressed by --quiet."
  [& parts]
  (when-not *quiet*
    (apply emit! parts)))

(defn warn
  "Prints a warning."
  [& parts]
  (apply emit! parts))

(defn error
  "Prints an error, prefixed with the program name."
  [& parts]
  (apply emit! "proserunner: " parts))

(defmacro ^:private terminal?
  "Expands to (.isTerminal c) when the compiling JDK has Console.isTerminal
  (JDK 22+), and to true otherwise, since older JDKs return nil from
  System/console when stdin or stdout is redirected. Deciding at compile
  time keeps the call reflection-free, which native-image needs."
  [c]
  (if (some #(= "isTerminal" (.getName ^java.lang.reflect.Method %))
            (.getMethods java.io.Console))
    `(.isTerminal ~(with-meta c {:tag 'java.io.Console}))
    true))

(defn interactive?
  "True when stdin and stdout are attached to a terminal.
  On JDK 22+ System/console can return a Console even when redirected,
  so isTerminal is the reliable check there."
  []
  (if-let [c (System/console)]
    (terminal? c)
    false))

(defn confirm?
  "Asks a yes/no question on the terminal. Defaults to no."
  [question]
  (let [^java.io.Writer w *err*]
    (.write w (str question " [y/N] "))
    (.flush w))
  (let [answer (some-> (read-line) string/trim string/lower-case)]
    (contains? #{"y" "yes"} answer)))
