(ns tasks.install
  "Installation helpers for proserunner binary.

  Provides composable functions for installing the binary and configuring PATH.
  Side effects are isolated to functions with ! suffix."
  (:require [babashka.fs :as fs]
            [babashka.process :refer [shell]]
            [clojure.string :as str]))

;;; Pure functions (no side effects)

(defn detect-shell
  "Detects the user's shell from $SHELL env var. Returns shell name or nil."
  []
  (when-let [shell (System/getenv "SHELL")]
    (last (str/split shell #"/"))))

(defn shell-config-file
  "Returns the config file path for a given shell and home directory."
  [shell-name home]
  (case shell-name
    "bash" (str home "/.bashrc")
    "zsh"  (str home "/.zshrc")
    "fish" (str home "/.config/fish/config.fish")
    nil))

(defn path-in-config?
  "Checks if .local/bin is already in the config file content."
  [config-content]
  (str/includes? config-content ".local/bin"))

(defn in-path?
  "Checks if directory is in PATH environment variable."
  [dir]
  (when-let [path (System/getenv "PATH")]
    (str/includes? path dir)))

(defn path-export-line
  "Returns the shell export line to add .local/bin to PATH."
  []
  "export PATH=\"$HOME/.local/bin:$PATH\"")

;;; I/O functions (side effects)

(defn read-config-file
  "Reads config file content, returns empty string if file doesn't exist."
  [config-file]
  (if (fs/exists? config-file)
    (slurp config-file)
    ""))

(defn add-path-to-config!
  "Appends PATH export to config file with a dated comment, so it's easy to
  find and remove later."
  [config-file]
  (spit config-file
        (str "\n# Added by proserunner installer on " (java.time.LocalDate/now) "\n"
             (path-export-line) "\n")
        :append true))

(defn prompt-yes-no
  "Prompts user with a yes/no question. Returns true if yes, false otherwise.
  Pressing Enter means yes; end of input (no one to answer, e.g. in a script)
  means no, so shell configs are never edited unattended."
  [message]
  (print (str message " [Y/n] "))
  (flush)
  (if-let [response (read-line)]
    (let [response (str/lower-case (str/trim response))]
      (or (empty? response) (= response "y") (= response "yes")))
    (do (println)
        false)))

(defn setup-path!
  "Prompts user to add PATH to shell config if needed.

  Returns true if PATH was already set up or successfully configured,
  false if user declined or setup failed."
  [home local-bin]
  (if (in-path? local-bin)
    true  ; Already in PATH
    (do
      (println "\n⚠ ~/.local/bin is not in your PATH")
      (if-let [shell-name (detect-shell)]
        (if-let [config-file (shell-config-file shell-name home)]
          (if (prompt-yes-no (str "\nAdd ~/.local/bin to your PATH in " config-file "?"))
            (try
              (let [existing-content (read-config-file config-file)]
                (if (path-in-config? existing-content)
                  (do
                    (println "✓ PATH configuration already exists in" config-file)
                    true)
                  (do
                    (add-path-to-config! config-file)
                    (println "✓ Added PATH to" config-file)
                    (println "\nTo use proserunner in this terminal, run:")
                    (println "  source" config-file)
                    (println "\nOr open a new terminal window.")
                    true)))
              (catch Exception e
                (println "⚠ Could not modify" config-file ":" (.getMessage e))
                (println "Please add this line manually:")
                (println " " (path-export-line))
                false))
            (do
              (println "\nTo add it manually, run:")
              (println " " (str "echo '" (path-export-line) "' >> " config-file))
              (println " " (str "source " config-file))
              false))
          (do
            (println "\nUnsupported shell:" shell-name)
            (println "Please add this line to your shell config:")
            (println " " (path-export-line))
            false))
        (do
          (println "\nCould not detect shell. To add it manually:")
          (println "  echo 'export PATH=\"$HOME/.local/bin:$PATH\"' >> ~/.bashrc  # or ~/.zshrc")
          (println "  source ~/.bashrc  # or ~/.zshrc")
          false)))))

(defn verify-installation!
  "Verifies installation by running binary with --version.

  Parameters:
  - binary-path: Full path to the installed binary
  - needs-path-setup?: Whether user needs to source config or restart terminal

  Returns true if verification succeeded, false otherwise."
  [binary-path needs-path-setup?]
  (println "\n=== Verifying installation ===\n")
  (try
    (let [result (shell {:out :string :continue true} binary-path "--version")]
      (if (zero? (:exit result))
        (do
          (println "✓ Installation successful!")
          (println (:out result))
          (when needs-path-setup?
            (println "\nNote: Restart your terminal or run 'source <config-file>' to use 'proserunner' globally."))
          true)
        (do
          (println "⚠ Warning: proserunner binary exists but failed to run")
          false)))
    (catch Exception e
      (println "⚠ Warning: Could not verify installation:" (.getMessage e))
      false)))

(defn install-to-local-bin!
  "Main installation workflow for ~/.local/bin installation.

  Handles:
  - Creating ~/.local/bin directory
  - Copying binary
  - Setting permissions
  - PATH configuration
  - Installation verification"
  []
  (println "\n=== Installing to ~/.local/bin ===\n")
  (when-not (fs/exists? "proserunner")
    (println "ERROR: proserunner not found. Build may have failed.")
    (System/exit 1))

  (let [home (System/getProperty "user.home")
        local-bin (str home "/.local/bin")
        target (str local-bin "/proserunner")
        was-in-path? (in-path? local-bin)]

    ;; Copy binary
    (fs/create-dirs local-bin)
    (fs/copy "proserunner" target {:replace-existing true})
    (fs/set-posix-file-permissions target "rwxr-xr-x")
    (println "✓ Installed to" target)

    ;; Setup PATH if needed
    (setup-path! home local-bin)

    ;; Verify installation
    (verify-installation! target (not was-in-path?))))

(defn install-to-system-bin!
  "Main installation workflow for /usr/local/bin installation (requires sudo).

  Handles:
  - Copying binary with sudo
  - Setting permissions
  - Installation verification"
  []
  (println "\n=== Installing to /usr/local/bin (requires sudo) ===\n")
  (when-not (fs/exists? "proserunner")
    (println "ERROR: proserunner not found. Build may have failed.")
    (System/exit 1))

  ;; Copy with sudo
  (shell "sudo cp proserunner /usr/local/bin/proserunner")
  (shell "sudo chmod +x /usr/local/bin/proserunner")
  (println "✓ Installed to /usr/local/bin/proserunner")

  ;; Verify installation (no PATH setup needed for /usr/local/bin)
  (verify-installation! "proserunner" false))
