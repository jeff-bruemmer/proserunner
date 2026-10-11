(ns hooks.test-helpers
  "Expansions of test macros, so clj-kondo sees what they bind.")

(defmacro with-temp-dirs
  "Binds each name in [[name prefix] ...], like let."
  [bindings & body]
  `(let [~@(apply concat bindings)] ~@body))
