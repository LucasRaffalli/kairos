//! Détecter le lot dont parle l'utilisateur, et décider quoi en faire. Fonctions **pures** : ni
//! entrée-sortie, ni horloge, ni base. L'appelant lit le message, la branche, les commits et
//! `.kairos.json`, résout chaque ID auprès du service (`resolve_monday_id`) puis appelle [`decide`].
//!
//! Aucun format n'est imposé : on cherche la **forme** d'un ID Monday (un lien, ou 9 à 12 chiffres),
//! pas un gabarit de commit. Dans le doute, on n'agit jamais : [`Decision::Doubt`] demande à
//! Claude de poser la question à l'utilisateur.

/// Longueur d'un ID Monday : 9 à 12 chiffres, sans zéro initial.
const ID_DIGITS: std::ops::RangeInclusive<usize> = 9..=12;
/// On ne regarde que ces derniers caractères (de la même ligne) devant un ID pour y trouver un repère.
const MARKER_WINDOW: usize = 24;
/// Au plus ce nombre d'IDs distincts : au-delà de un c'est déjà un doute, et un texte énorme ne doit
/// jamais ralentir un hook.
pub const MAX_IDS: usize = 20;
/// Mots qui introduisent un ID (« ticket 123 », « task-123 »…), entiers et au singulier.
const MARKERS: [&str; 5] = ["ticket", "tâche", "tache", "task", "item"];
const MARKERS_WITH_SEPARATOR: [&str; 2] = ["monday", "pulse"];

/// Un ID trouvé dans un texte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundId {
    pub id: String,
    /// Sûr par sa forme : un lien Monday, ou un repère juste devant (« monday », « ticket »,
    /// « tâche », « task », « item », « pulse », ou `#`).
    pub sure_by_shape: bool,
}

/// Les IDs trouvés, dédoublonnés, au plus [`MAX_IDS`] distincts.
#[derive(Default)]
struct Collector {
    /// (position dans le texte, ID)
    found: Vec<(usize, FoundId)>,
    index: std::collections::HashMap<String, usize>,
}

impl Collector {
    fn add(&mut self, at: usize, id: &str, sure_by_shape: bool) {
        match self.index.get(id) {
            Some(&i) => self.found[i].1.sure_by_shape |= sure_by_shape,
            None if self.index.len() < MAX_IDS => {
                self.index.insert(id.to_string(), self.found.len());
                self.found.push((
                    at,
                    FoundId {
                        id: id.to_string(),
                        sure_by_shape,
                    },
                ));
            }
            None => {}
        }
    }

    /// Dans l'ordre d'apparition.
    fn into_ids(mut self) -> Vec<FoundId> {
        self.found.sort_by_key(|(at, _)| *at);
        self.found.into_iter().map(|(_, f)| f).collect()
    }
}

/// Tous les IDs Monday d'un texte, dans l'ordre d'apparition, sans doublon, au plus [`MAX_IDS`].
pub fn extract_ids(text: &str) -> Vec<FoundId> {
    let mut collector = Collector::default();
    let mut link_spans: Vec<(usize, usize)> = Vec::new();

    // Les liens d'abord : on n'y lit que l'ID de l'élément, jamais l'ID du tableau ou de la vue.
    let mut start = None;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        match (start, c.is_whitespace()) {
            (None, false) => start = Some(i),
            (Some(from), true) => {
                let token = &text[from..i];
                let lower = token.to_lowercase();
                if lower.contains("monday.com") && link_spans.len() < 64 {
                    link_spans.push((from, i));
                    for key in ["/pulses/", "/items/"] {
                        let mut search = 0;
                        while let Some(at) = lower[search..].find(key) {
                            let digits_at = search + at + key.len();
                            let digits: String = lower[digits_at..]
                                .chars()
                                .take_while(char::is_ascii_digit)
                                .collect();
                            if valid_shape(&digits) {
                                collector.add(from + digits_at.min(token.len()), &digits, true);
                            }
                            search = digits_at;
                        }
                    }
                }
                start = None;
            }
            _ => {}
        }
    }

    // Puis les nombres nus.
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut previous_marked_end: Option<usize> = None;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let from = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let digits = &text[from..i];
        if link_spans.iter().any(|&(a, b)| from >= a && from < b) || !valid_shape(digits) {
            continue;
        }
        let before = text[..from].chars().next_back();
        let after = text[i..].chars().next();
        if before.is_some_and(char::is_alphabetic) || after.is_some_and(char::is_alphabetic) {
            continue;
        }
        let marked = has_marker_before(text, from)
            || previous_marked_end.is_some_and(|end| {
                text[end..from]
                    .chars()
                    .all(|c| c.is_whitespace() && c != '\n' || matches!(c, ',' | ';' | '&' | '+'))
            });
        previous_marked_end = marked.then_some(i);
        collector.add(from, digits, marked);
    }

    collector.into_ids()
}

fn valid_shape(digits: &str) -> bool {
    ID_DIGITS.contains(&digits.len())
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Un repère introduit-il l'ID qui commence à `at` ? Seuls les derniers caractères de la même ligne
/// comptent. Il faut `#` collé à l'ID (et pas collé à un mot), ou un mot-repère entier juste devant
/// (« ticket 123 », « task-123 », « ticket n° 123 », « monday: 123 »). « Monday 123 » (le jour de
/// la semaine), « multitask 123 » ou « #42 date de 123 » ne comptent pas.
fn has_marker_before(text: &str, at: usize) -> bool {
    let mut reversed: Vec<char> = Vec::with_capacity(MARKER_WINDOW);
    for c in text[..at].chars().rev().take(MARKER_WINDOW) {
        if c == '\n' {
            break;
        }
        reversed.push(c);
    }
    let window: String = reversed.into_iter().rev().collect();

    if let Some(rest) = window.strip_suffix('#') {
        return !rest.chars().next_back().is_some_and(char::is_alphanumeric);
    }
    let separators = [' ', '\t', ':', '=', '-', '°', 'º', '#'];
    let trimmed = window.trim_end_matches(separators);
    let separator = &window[trimmed.len()..];
    // « ticket n° 123 » : le « n » du numéro n'est pas un mot.
    let trimmed = trimmed
        .strip_suffix(" n")
        .or_else(|| trimmed.strip_suffix(" no"))
        .map_or(trimmed, |t| t.trim_end_matches(separators));
    let word: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>()
        .to_lowercase();
    if MARKERS.contains(&word.as_str()) {
        return true;
    }
    // « monday » et « pulse » seuls sont ambigus (le lundi) : il leur faut un `:` ou un `=`.
    MARKERS_WITH_SEPARATOR.contains(&word.as_str()) && separator.contains([':', '='])
}

/// Le réglage `claude_autonomy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autonomy {
    Ask,
    Auto,
}

/// D'où vient l'ID retenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Message,
    BranchOrCommits,
    Config,
}

/// Un ID déjà résolu par l'appelant : le parent (une sous-tâche mémorisée est ramenée à son parent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub parent: String,
    pub subtask: Option<String>,
    pub label: Option<String>,
    /// Sûr : sûr par sa forme, reconnu par Kairos, ou venu de `.kairos.json`.
    pub sure: bool,
    /// Kairos ne connaissait pas ce parent.
    pub new_parent: bool,
}

impl Resolved {
    fn describe(&self) -> String {
        match &self.label {
            Some(label) => format!("{label} ({})", self.parent),
            None => self.parent.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecideInput {
    /// Le parent du lot en cours (ou en pause), s'il y en a un.
    pub running_parent: Option<String>,
    /// Un lot tourne ou est en pause, même sans parent Monday (une réunion, une tâche nommée).
    pub running_lot: bool,
    pub autonomy: Autonomy,
    /// Les parents cités dans le message de l'utilisateur.
    pub message: Vec<Resolved>,
    /// Les parents de la branche et des derniers commits, du plus récent au plus ancien.
    pub branch_or_commits: Vec<Resolved>,
    /// La branche et les commits citent chacun des parents, sans aucun en commun.
    pub conflicting_branch_and_commits: bool,
    pub config: Option<Resolved>,
}

/// Ce qu'il faut lancer ou signaler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub parent: String,
    pub subtask: Option<String>,
    pub label: Option<String>,
    pub new_parent: bool,
    pub origin: Origin,
    /// Un autre lot tourne : agir revient à basculer.
    pub replaces_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Nothing,
    /// Lancer ou basculer maintenant (`auto`, ID sûr).
    Act(Plan),
    /// Ne rien faire, dire à Claude ce qui a été détecté (`ask`, ID sûr).
    Inform(Plan),
    /// Ne rien faire, dire à Claude de demander à l'utilisateur.
    Doubt(String),
}

pub fn decide(input: &DecideInput) -> Decision {
    let running = input.running_parent.as_deref();
    let busy = running.is_some() || input.running_lot;

    if !input.message.is_empty() {
        if input.message.len() > 1 {
            return doubt(&format!(
                "le message cite plusieurs tâches ({})",
                list(&input.message)
            ));
        }
        let found = &input.message[0];
        if !found.sure {
            return doubt(&format!(
                "le message contient le numéro {}, que Kairos ne reconnaît pas ; ce peut être autre chose qu'un ID Monday",
                found.parent
            ));
        }
        if running == Some(found.parent.as_str()) {
            return Decision::Nothing;
        }
        return act(input, found, Origin::Message, busy);
    }

    if !input.branch_or_commits.is_empty() {
        let set = &input.branch_or_commits;
        if input.conflicting_branch_and_commits {
            return doubt(&format!(
                "la branche et les commits ne pointent pas vers les mêmes tâches (candidats : {})",
                list(set)
            ));
        }
        if let Some(run) = running {
            if set.iter().any(|r| r.parent == run) {
                return Decision::Nothing;
            }
            return doubt(&format!(
                "Kairos suit {run}, mais la branche et les commits concernent {}",
                list(set)
            ));
        }
        if busy {
            return doubt(&format!(
                "Kairos suit un lot sans ID Monday, mais la branche et les commits concernent {}",
                list(set)
            ));
        }
        if set.len() > 1 {
            return doubt(&format!(
                "la branche et les commits concernent plusieurs tâches ({})",
                list(set)
            ));
        }
        if !set[0].sure {
            return doubt(&format!(
                "la branche ou les commits contiennent le numéro {}, que Kairos ne reconnaît pas ; ce peut être autre chose qu'un ID Monday",
                set[0].parent
            ));
        }
        return act(input, &set[0], Origin::BranchOrCommits, false);
    }

    if let Some(config) = &input.config {
        return match running {
            Some(run) if run == config.parent => Decision::Nothing,
            Some(run) => doubt(&format!(
                "Kairos suit {run}, mais .kairos.json indique {}",
                config.describe()
            )),
            None if busy => doubt(&format!(
                "Kairos suit un lot sans ID Monday, mais .kairos.json indique {}",
                config.describe()
            )),
            None => act(input, config, Origin::Config, false),
        };
    }

    Decision::Nothing
}

fn act(input: &DecideInput, found: &Resolved, origin: Origin, replaces_running: bool) -> Decision {
    let plan = Plan {
        parent: found.parent.clone(),
        subtask: found.subtask.clone(),
        label: found.label.clone(),
        new_parent: found.new_parent,
        origin,
        replaces_running,
    };
    match input.autonomy {
        Autonomy::Auto => Decision::Act(plan),
        Autonomy::Ask => Decision::Inform(plan),
    }
}

fn list(found: &[Resolved]) -> String {
    found
        .iter()
        .map(Resolved::describe)
        .collect::<Vec<_>>()
        .join(", ")
}

fn doubt(why: &str) -> Decision {
    Decision::Doubt(format!(
        "Doute : {why}. Demande à l'utilisateur quelle tâche est la bonne avant d'appeler `start` ou `switch`. Ne devine pas."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "12406850311";
    const B: &str = "12406856533";
    const C: &str = "12406850412";

    fn ids(text: &str) -> Vec<(String, bool)> {
        extract_ids(text)
            .into_iter()
            .map(|f| (f.id, f.sure_by_shape))
            .collect()
    }

    fn sure(id: &str) -> (String, bool) {
        (id.to_string(), true)
    }

    fn unsure(id: &str) -> (String, bool) {
        (id.to_string(), false)
    }

    #[test]
    fn every_commit_style_gives_every_id() {
        assert_eq!(ids(&format!("[Monday #{A}]")), [sure(A)]);
        assert_eq!(ids(&format!("[Monday #{A}, #{B}]")), [sure(A), sure(B)]);
        assert_eq!(
            ids(&format!("[Monday #{A}] [Monday #{B}]")),
            [sure(A), sure(B)]
        );
        assert_eq!(ids(&format!("closes #{A} #{B}")), [sure(A), sure(B)]);
        assert_eq!(ids(&format!("monday: {A}")), [sure(A)]);
        assert_eq!(ids(&format!("Ticket {A}\nTâche {B}")), [sure(A), sure(B)]);
        // Sans repère : trouvés, mais pas sûrs par leur forme.
        assert_eq!(
            ids(&format!("fix(bundle): {A} {B} {C}")),
            [unsure(A), unsure(B), unsure(C)]
        );
        // Une liste dont seul le premier porte le repère : les suivants le partagent.
        assert_eq!(ids(&format!("[Monday #{A}, {B}]")), [sure(A), sure(B)]);
    }

    #[test]
    fn a_monday_link_gives_its_item_id_and_nothing_else() {
        let url = format!("https://acme.monday.com/boards/123456789/pulses/{A}");
        assert_eq!(ids(&url), [sure(A)]);
        assert_eq!(
            ids(&format!("voir https://acme.monday.com/boards/123456789/views/987654321/pulses/{B}?x=1 merci")),
            [sure(B)]
        );
        assert_eq!(
            ids(&format!("https://acme.monday.com/items/{C}")),
            [sure(C)]
        );
    }

    #[test]
    fn the_same_id_twice_counts_once_and_keeps_its_best_certainty() {
        assert_eq!(ids(&format!("{A} puis #{A}")), [sure(A)]);
    }

    #[test]
    fn traps_are_ignored_or_unsure() {
        // Un horodatage de 10 chiffres a la bonne forme mais aucun repère.
        assert_eq!(ids("à 1792774800"), [unsure("1792774800")]);
        for ignored in [
            "0612345678",
            "2026-10-24",
            "v1.2.3",
            "issue 12345",
            "abc1234567890xyz",
            "x1234567890",
            "1234567890y",
            "1234567890123",
            "",
        ] {
            assert_eq!(ids(ignored), Vec::<(String, bool)>::new(), "{ignored:?}");
        }
        // Séparé par des tirets ou des barres : toujours un ID.
        assert_eq!(ids(&format!("feature/{A}-bundle")), [unsure(A)]);
    }

    fn resolved(parent: &str) -> Resolved {
        Resolved {
            parent: parent.into(),
            subtask: None,
            label: None,
            sure: true,
            new_parent: false,
        }
    }

    fn input(running: Option<&str>, autonomy: Autonomy) -> DecideInput {
        DecideInput {
            running_parent: running.map(String::from),
            running_lot: running.is_some(),
            autonomy,
            message: vec![],
            branch_or_commits: vec![],
            conflicting_branch_and_commits: false,
            config: None,
        }
    }

    fn plan(decision: &Decision) -> &Plan {
        match decision {
            Decision::Act(p) | Decision::Inform(p) => p,
            other => panic!("attendu une action ou une information, reçu {other:?}"),
        }
    }

    fn doubt(decision: &Decision) -> &str {
        match decision {
            Decision::Doubt(text) => text,
            other => panic!("attendu un doute, reçu {other:?}"),
        }
    }

    fn assert_asks(text: &str) {
        assert!(text.starts_with("Doute : "), "{text}");
        assert!(text.contains("Demande à l'utilisateur"), "{text}");
        assert!(text.contains("Ne devine pas"), "{text}");
    }

    #[test]
    fn a_sure_id_in_the_message_starts_a_lot_in_auto_and_informs_in_ask() {
        for (autonomy, acts) in [(Autonomy::Auto, true), (Autonomy::Ask, false)] {
            let mut i = input(None, autonomy);
            i.message = vec![resolved(A)];
            let d = decide(&i);
            assert_eq!(matches!(d, Decision::Act(_)), acts, "{d:?}");
            assert_eq!(matches!(d, Decision::Inform(_)), !acts, "{d:?}");
            let p = plan(&d);
            assert_eq!(p.parent, A);
            assert_eq!(p.origin, Origin::Message);
            assert!(!p.replaces_running);
        }
    }

    #[test]
    fn a_sure_id_in_the_message_replaces_another_running_lot() {
        let mut i = input(Some(B), Autonomy::Auto);
        i.message = vec![resolved(A)];
        let d = decide(&i);
        assert!(matches!(d, Decision::Act(_)), "{d:?}");
        assert!(plan(&d).replaces_running);
        i.autonomy = Autonomy::Ask;
        assert!(matches!(decide(&i), Decision::Inform(_)));
    }

    #[test]
    fn the_running_lot_cited_again_changes_nothing() {
        for autonomy in [Autonomy::Auto, Autonomy::Ask] {
            let mut i = input(Some(A), autonomy);
            i.message = vec![resolved(A)];
            assert!(matches!(decide(&i), Decision::Nothing));
        }
    }

    #[test]
    fn two_parents_in_the_message_are_a_doubt_that_lists_both() {
        for autonomy in [Autonomy::Auto, Autonomy::Ask] {
            let mut i = input(None, autonomy);
            i.message = vec![resolved(A), resolved(B)];
            let d = decide(&i);
            assert_asks(doubt(&d));
            assert!(doubt(&d).contains(A) && doubt(&d).contains(B));
        }
    }

    #[test]
    fn a_bare_unknown_number_is_a_doubt_never_an_action() {
        let mut i = input(None, Autonomy::Auto);
        i.message = vec![Resolved {
            sure: false,
            new_parent: true,
            ..resolved("1792774800")
        }];
        let d = decide(&i);
        assert_asks(doubt(&d));
        assert!(doubt(&d).contains("1792774800"));
    }

    #[test]
    fn a_branch_with_three_parents_is_silent_when_the_running_lot_is_one_of_them() {
        for autonomy in [Autonomy::Auto, Autonomy::Ask] {
            let mut i = input(Some(B), autonomy);
            i.branch_or_commits = vec![resolved(A), resolved(B), resolved(C)];
            assert!(matches!(decide(&i), Decision::Nothing));
        }
    }

    #[test]
    fn a_branch_with_three_parents_and_nothing_running_asks_which() {
        let mut i = input(None, Autonomy::Auto);
        i.branch_or_commits = vec![resolved(A), resolved(B), resolved(C)];
        let d = decide(&i);
        assert_asks(doubt(&d));
        let text = doubt(&d);
        let (a, b, c) = (
            text.find(A).unwrap(),
            text.find(B).unwrap(),
            text.find(C).unwrap(),
        );
        assert!(
            a < b && b < c,
            "l'ordre des candidats est conservé : {text}"
        );
    }

    #[test]
    fn a_running_lot_outside_the_branch_parents_is_a_doubt() {
        let mut i = input(Some(C), Autonomy::Auto);
        i.branch_or_commits = vec![resolved(A), resolved(B)];
        let d = decide(&i);
        assert_asks(doubt(&d));
        assert!(doubt(&d).contains(C) && doubt(&d).contains(A) && doubt(&d).contains(B));
    }

    #[test]
    fn conflicting_branch_and_commits_are_a_doubt() {
        let mut i = input(None, Autonomy::Auto);
        i.branch_or_commits = vec![resolved(A), resolved(B)];
        i.conflicting_branch_and_commits = true;
        assert_asks(doubt(&decide(&i)));
    }

    #[test]
    fn a_single_known_parent_on_the_branch_starts_a_lot_when_idle() {
        let mut i = input(None, Autonomy::Auto);
        i.branch_or_commits = vec![resolved(A)];
        let d = decide(&i);
        assert!(matches!(d, Decision::Act(_)), "{d:?}");
        assert_eq!(plan(&d).origin, Origin::BranchOrCommits);
        i.autonomy = Autonomy::Ask;
        assert!(matches!(decide(&i), Decision::Inform(_)));
    }

    #[test]
    fn a_kairos_json_starts_a_lot_with_its_label_and_subtask() {
        let mut i = input(None, Autonomy::Auto);
        i.config = Some(Resolved {
            label: Some("Bundle".into()),
            subtask: Some("Back".into()),
            ..resolved(A)
        });
        let d = decide(&i);
        let p = plan(&d);
        assert!(matches!(d, Decision::Act(_)));
        assert_eq!(p.label.as_deref(), Some("Bundle"));
        assert_eq!(p.subtask.as_deref(), Some("Back"));
        assert_eq!(p.origin, Origin::Config);
        i.autonomy = Autonomy::Ask;
        assert!(matches!(decide(&i), Decision::Inform(_)));
    }

    #[test]
    fn the_message_wins_over_the_branch_and_the_config() {
        let mut i = input(None, Autonomy::Auto);
        i.message = vec![resolved(A)];
        i.branch_or_commits = vec![resolved(B)];
        i.config = Some(resolved(C));
        assert_eq!(plan(&decide(&i)).parent, A);
    }

    #[test]
    fn a_running_lot_without_a_parent_is_never_replaced_by_the_branch_or_the_config() {
        for autonomy in [Autonomy::Auto, Autonomy::Ask] {
            let mut i = input(None, autonomy);
            i.running_lot = true;
            i.branch_or_commits = vec![resolved(A)];
            assert_asks(doubt(&decide(&i)));
            let mut i = input(None, autonomy);
            i.running_lot = true;
            i.config = Some(resolved(A));
            assert_asks(doubt(&decide(&i)));
        }
        // Dans le message, un ID sûr remplace bien un lot sans parent.
        let mut i = input(None, Autonomy::Auto);
        i.running_lot = true;
        i.message = vec![resolved(A)];
        assert!(plan(&decide(&i)).replaces_running);
    }

    #[test]
    fn a_marker_must_really_introduce_the_number() {
        // Faux repères : un autre nombre derrière le `#`, un mot dont le repère n'est qu'un morceau,
        // le jour de la semaine, un titre Markdown, un `#` collé à une lettre.
        for (text, id) in [
            ("la PR #42 date de 1792774800", "1792774800"),
            ("multitask 1792774800", "1792774800"),
            ("items: 123456789 unités", "123456789"),
            ("Monday 1792774800", "1792774800"),
            ("## Logs 1792774800", "1792774800"),
            ("c#1234567890", "1234567890"),
            ("# 1792774800", "1792774800"),
            ("retask 1792774800", "1792774800"),
        ] {
            assert_eq!(ids(text), [unsure(id)], "{text}");
        }
        // Vrais repères.
        for text in [
            format!("ticket n° {A}"),
            format!("Ticket: {A}"),
            format!("tâche {A}"),
            format!("TASK-{A}"),
            format!("monday: {A}"),
            format!("Monday #{A}"),
            format!("item {A}"),
            format!("pulse={A}"),
            format!("#{A}"),
        ] {
            assert_eq!(ids(&text), [sure(A)], "{text}");
        }
        // Un lien en majuscules.
        assert_eq!(
            ids(&format!("https://x.monday.com/boards/1/Pulses/{A}")),
            [sure(A)]
        );
    }

    #[test]
    fn extraction_stays_fast_and_bounded_on_a_huge_text() {
        let distinct: String = (0..90_000u64)
            .map(|i| format!("{} ", 1_000_000_000 + i))
            .collect();
        let started = std::time::Instant::now();
        let found = extract_ids(&distinct);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(found.len(), MAX_IDS);
        let one_line = "x".repeat(1_000_000) + " 12406850311";
        let started = std::time::Instant::now();
        assert_eq!(extract_ids(&one_line).len(), 1);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn no_source_changes_nothing() {
        assert!(matches!(
            decide(&input(None, Autonomy::Auto)),
            Decision::Nothing
        ));
        assert!(matches!(
            decide(&input(Some(A), Autonomy::Ask)),
            Decision::Nothing
        ));
    }
}
