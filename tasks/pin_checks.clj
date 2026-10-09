(ns tasks.pin-checks
  "Prints the default-checks pin for src/proserunner/config.clj."
  (:require [babashka.http-client :as http]
            [cheshire.core :as json]
            [proserunner.config :as conf]))

(def ^:private repo "jeff-bruemmer/proserunner-default-checks")

(defn pin-checks
  "Resolves `ref` (a branch, tag, or commit; default main) to a commit,
  downloads that commit's archive, and prints the two defs to paste into
  config.clj."
  [& [ref]]
  (let [ref (or ref "main")
        sha (-> (http/get (str "https://api.github.com/repos/" repo "/commits/" ref))
                :body
                (json/parse-string true)
                :sha)
        zip (:body (http/get (str "https://github.com/" repo "/archive/" sha ".zip")
                             {:as :bytes}))]
    (println (format "(def default-checks-ref \"%s\")" sha))
    (println (format "(def default-checks-sha256 \"%s\")" (conf/archive-digest zip)))))
