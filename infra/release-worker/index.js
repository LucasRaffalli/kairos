// Sert le bucket R2 « kairos-releases » sur release.lucasraffalli.com/kairos/*.
//
// La route Cloudflare est limitée à ce chemin : /charon/* continue d'aller au
// serveur de Charon, ce Worker ne le voit jamais. Il ne fait que lire un objet
// par sa clé : jamais de listing du bucket, jamais d'écriture (les fichiers
// arrivent par `npm run deploy`, avec les identifiants de Lucas).
const PREFIX = '/kairos/';

export default {
  async fetch(request, env) {
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      return new Response('Méthode non autorisée', { status: 405, headers: { Allow: 'GET, HEAD' } });
    }

    const { pathname } = new URL(request.url);
    if (!pathname.startsWith(PREFIX)) {
      return notFound();
    }

    let key;
    try {
      key = decodeURIComponent(pathname.slice(PREFIX.length));
    } catch {
      return notFound();
    }
    // Une clé vide (listing), un `..` ou un `//` : jamais un fichier de release.
    if (!key || key.split('/').some((part) => part === '' || part === '.' || part === '..')) {
      return notFound();
    }

    const object = await env.RELEASES.get(key);
    if (!object) {
      return notFound();
    }

    const headers = new Headers();
    object.writeHttpMetadata?.(headers);
    headers.set('ETag', object.httpEtag ?? `"${object.etag}"`);
    if (key === 'latest.json') {
      // Le manifeste ne se met JAMAIS en cache : c'est lui qui annonce la version.
      headers.set('Content-Type', 'application/json');
      headers.set('Cache-Control', 'no-cache');
    } else if (!headers.has('Cache-Control')) {
      // Les fichiers portent leur version dans leur nom : une heure de cache est sûre.
      headers.set('Cache-Control', 'public, max-age=3600');
    }
    return new Response(request.method === 'HEAD' ? null : object.body, { status: 200, headers });
  },
};

function notFound() {
  return new Response('Introuvable', { status: 404, headers: { 'Cache-Control': 'no-store' } });
}
