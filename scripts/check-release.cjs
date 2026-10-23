// Cohérence d'une release : la version est écrite à trois endroits
// (package.json, tauri.conf.json, Cargo.toml) et le changelog doit avoir une
// entrée pour elle. Avec un tag en argument (v1.3.0), il doit aussi coïncider.
// La traduction anglaise (changelog.en.json) doit couvrir chaque version, note
// pour note : une note ajoutée en français seulement décalerait toutes les
// suivantes, l'anglais étant apparié au français par position.
// Lancé par ci.yml à chaque push et par release.yml avant le moindre build.
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const read = (p) => fs.readFileSync(path.join(root, p), "utf8");

const versions = {
  "package.json": JSON.parse(read("package.json")).version,
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  "src-tauri/Cargo.toml": /^version\s*=\s*"([^"]+)"/m.exec(read("src-tauri/Cargo.toml"))?.[1],
};
const changelog = JSON.parse(read("src/assets/changelog.json"));
const english = JSON.parse(read("src/assets/changelog.en.json"));

const errors = [];
const unique = new Set(Object.values(versions));
if (unique.size !== 1) {
  errors.push(
    "Versions différentes : " +
      Object.entries(versions)
        .map(([f, v]) => `${f}=${v}`)
        .join(", "),
  );
}
const version = versions["package.json"];
if (!changelog.some((e) => e.version === version)) {
  errors.push(`changelog.json n'a pas d'entrée pour la version ${version}`);
}
for (const entry of changelog) {
  const tr = english[entry.version];
  if (!tr) {
    errors.push(`changelog.en.json n'a pas de traduction pour la version ${entry.version}`);
  } else if (!Array.isArray(tr.notes) || tr.notes.length !== entry.notes.length) {
    errors.push(
      `changelog.en.json, version ${entry.version} : ${tr.notes?.length ?? 0} note(s) traduite(s) pour ${entry.notes.length} en français`,
    );
  } else if (tr.notes.some((note) => typeof note.text !== "string" || !note.text.trim())) {
    errors.push(`changelog.en.json, version ${entry.version} : une note traduite n'a pas de texte`);
  } else {
    // Les champs facultatifs aussi : un « Comment l'essayer » laissé en
    // français au milieu d'une note anglaise passerait sans un mot.
    entry.notes.forEach((note, i) => {
      if (note.how && !tr.notes[i].how) {
        errors.push(`changelog.en.json, version ${entry.version}, note ${i + 1} : le « how » n'est pas traduit`);
      }
    });
    for (const field of ["title", "before", "next"]) {
      if (entry[field] && !tr[field]) {
        errors.push(`changelog.en.json, version ${entry.version} : « ${field} » n'est pas traduit`);
      }
    }
  }
}
for (const version of Object.keys(english)) {
  if (!changelog.some((e) => e.version === version)) {
    errors.push(`changelog.en.json traduit une version ${version} absente du changelog`);
  }
}
const tag = process.argv[2];
if (tag && tag.replace(/^v/, "") !== version) {
  errors.push(`Le tag ${tag} ne correspond pas à la version ${version}`);
}

if (errors.length) {
  console.error(errors.map((e) => "✗ " + e).join("\n"));
  process.exit(1);
}
console.log(`✓ version ${version} cohérente${tag ? ` avec le tag ${tag}` : ""}`);
