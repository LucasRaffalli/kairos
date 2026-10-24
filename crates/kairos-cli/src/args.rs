//! Les arguments de la ligne de commande.

use clap::{Args, Parser, Subcommand, ValueEnum};
use kairos_core::model::Source;

#[derive(Parser)]
#[command(
    name = "kairos",
    about = "Kairos, chronomètre de barre de menus pour Monday"
)]
pub struct Cli {
    /// Réponse structurée (JSON) au lieu du texte ; les erreurs aussi.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SourceArg {
    Manual,
    ClaudeCode,
    ClaudeDesktop,
}

impl From<SourceArg> for Source {
    fn from(s: SourceArg) -> Source {
        match s {
            SourceArg::Manual => Source::Manual,
            SourceArg::ClaudeCode => Source::ClaudeCode,
            SourceArg::ClaudeDesktop => Source::ClaudeDesktop,
        }
    }
}

/// Ce qui désigne un lot, pour `start` et `switch`.
#[derive(Args)]
pub struct LotArgs {
    /// ID Monday du parent (chiffres).
    #[arg(long)]
    pub parent: Option<String>,
    /// Nom affiché du parent, mémorisé avec son ID.
    #[arg(long)]
    pub label: Option<String>,
    /// Nom de la tâche, quand il n'y a pas d'ID Monday.
    #[arg(long)]
    pub name: Option<String>,
    /// Sous-tâche (par défaut : le poste principal, avec un ID).
    #[arg(long)]
    pub subtask: Option<String>,
    /// ID Monday de la sous-tâche, collé à la main.
    #[arg(long)]
    pub subtask_id: Option<String>,
    /// Heure de début : 13:30, AAAA-MM-JJTHH:MM ou RFC 3339 (vide : maintenant).
    #[arg(long)]
    pub at: Option<String>,
    #[arg(long, value_enum, default_value = "manual")]
    pub source: SourceArg,
}

#[derive(Subcommand)]
pub enum Command {
    /// Lance un lot.
    Start(LotArgs),
    /// Arrête le lot en cours.
    Stop {
        #[arg(long)]
        at: Option<String>,
    },
    /// Met le lot en pause.
    Pause,
    /// Reprend le lot en pause.
    Resume,
    /// Arrête le lot en cours et lance le suivant, à la même seconde.
    Switch(LotArgs),
    /// Ce que fait Kairos maintenant.
    Status,
    /// Les entrées d'une journée.
    Today {
        #[arg(long)]
        date: Option<String>,
    },
    /// La ventilation d'une journée et la ligne à saisir dans Monday.
    Report {
        #[arg(long)]
        date: Option<String>,
    },
    /// Clôture une journée.
    CloseDay {
        #[arg(long)]
        date: Option<String>,
        /// Arrête d'abord le lot en cours.
        #[arg(long)]
        stop: bool,
    },
    /// Note que la journée est saisie dans Monday.
    MarkDeclared {
        #[arg(long)]
        date: Option<String>,
    },
    /// Ajoute un temps oublié.
    Add {
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        subtask: Option<String>,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        /// Jour de référence des heures HH:MM (aujourd'hui par défaut).
        #[arg(long)]
        date: Option<String>,
    },
    /// Corrige une entrée.
    Edit {
        id: i64,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        subtask: Option<String>,
        #[arg(long)]
        date: Option<String>,
    },
    /// Supprime une entrée.
    Delete { id: i64 },
    /// Mémorise l'ID Monday d'une sous-tâche d'un parent.
    SetSubtaskId {
        #[arg(long)]
        parent: String,
        #[arg(long)]
        subtask: String,
        #[arg(long)]
        id: String,
    },
    /// Lit ou écrit un réglage (main_subtask, rounding_minutes, claude_autonomy).
    Config { key: String, value: Option<String> },
    /// Lance le serveur MCP sur l'entrée et la sortie standard (pour Claude Code et Claude Desktop).
    Mcp,
    /// Affiche la version.
    Version,
}
