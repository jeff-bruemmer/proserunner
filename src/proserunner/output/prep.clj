(ns proserunner.output.prep
  "Issue preparation and sorting."
  (:gen-class)
  (:require [clojure.string :as string]
            [proserunner.console :as console]
            [proserunner.fmt :as fmt]))

(set! *warn-on-reflection* true)

(defn time-elapsed
  "Prints elapsed time to stderr if timer is enabled, so it never mixes
  into piped output."
  [{:keys [timer start-time]}]
  (let [end-time (System/currentTimeMillis)]
    (when timer (console/warn "Completed in " (- end-time start-time) " ms."))))

(defn prep
  "Prepares results for printing by merging line data with each issue."
  [{:keys [line-num issues text]}]
  (reduce (fn [l issue]
            (let [{:keys [file name specimen col-num message kind]} issue]
              (conj l {:file file
                       :line-num line-num
                       :col-num (inc col-num)
                       :specimen (string/trim specimen)
                       :line-text (string/trim text)
                       :name (string/capitalize (string/replace name "-" " "))
                       :message (fmt/sentence-dress message)
                       :kind kind})))
          []
          issues))

;;; Column alignment for grouped output

(def ^:private max-specimen-column
  "Specimens wider than this don't widen the specimen column; their own row
  overflows instead of pushing every message far to the right."
  40)

(def ^:private column-gap
  "Separator between aligned columns."
  "  ")

(defn- pad-right
  "Left-justifies s in a field of width spaces."
  [s width]
  (let [s (str s)]
    (str s (apply str (repeat (- width (count s)) \space)))))

(defn- pad-left
  "Right-justifies s in a field of width spaces."
  [s width]
  (let [s (str s)]
    (str (apply str (repeat (- width (count s)) \space)) s)))

(defn- quoted
  [specimen]
  (str \" specimen \"))

(defn column-widths
  "Widths for the aligned columns of the grouped output. Measured across every
  issue in the run, so separate file groups share the same columns.
  Takes a seq of [issue-num issue] pairs."
  [numbered]
  {:num (apply max 0 (map (fn [[n _]] (count (str "[" n "]"))) numbered))
   :line (apply max 0 (map (fn [[_ issue]] (count (str (:line-num issue)))) numbered))
   :col (apply max 0 (map (fn [[_ issue]] (count (str (:col-num issue)))) numbered))
   ;; A single runaway specimen shouldn't shove every message off-screen, so
   ;; specimens past the cap are excluded and simply overflow their column.
   :specimen (->> numbered
                  (map (fn [[_ issue]] (count (quoted (:specimen issue)))))
                  (filter #(<= % max-specimen-column))
                  (apply max 0))})

(defn issue-str
  "Creates a simplified result string for grouped results.
  Optional issue-num parameter adds a numbered prefix. Optional widths map (see
  column-widths) pads the columns into alignment; without it the fields are just
  separated by spaces."
  ([issue] (issue-str nil issue nil))
  ([issue-num issue] (issue-str issue-num issue nil))
  ([issue-num {:keys [line-num col-num specimen message]} widths]
   (let [{:keys [num line col] specimen-width :specimen} (or widths {})
         line-width (or line 0)
         col-width (or col 0)
         ;; Right-justify the line number and left-justify the column number so
         ;; the colons line up down the page.
         position (str (pad-left line-num line-width) ":" col-num)]
     (string/join column-gap
       (cond-> []
         issue-num (conj (pad-right (str "[" issue-num "]") (or num 0)))
         true (conj (pad-right position (+ line-width 1 col-width)))
         true (conj (str (pad-right (quoted specimen) (or specimen-width 0))
                         " -> " message)))))))
