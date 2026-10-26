// Local-only preferences; no analytics, network calls or tracking cookies.
(() => {
  const supported = ['en', 'fr', 'es'];
  const applyLanguage = (language) => {
    const lang = supported.includes(language) ? language : 'en';
    document.documentElement.lang = lang;
    document.querySelectorAll('[data-en]').forEach((element) => {
      const text = element.dataset[lang] || element.getAttribute('data-' + lang) || element.dataset.en;
      if (text) {
        element.textContent = text;
      }
    });
    document.querySelectorAll('[data-language]').forEach((button) => {
      button.setAttribute('aria-pressed', String(button.dataset.language === lang));
    });
    try { localStorage.setItem('regta-language', lang); } catch (_) { /* Private browsing. */ }
  };
  document.querySelectorAll('[data-language]').forEach((button) => {
    button.addEventListener('click', (e) => {
      e.preventDefault();
      applyLanguage(button.dataset.language || button.getAttribute('data-language'));
    });
  });
  let language = 'en';
  try { language = localStorage.getItem('regta-language') || 'en'; } catch (_) { /* English fallback. */ }
  applyLanguage(language);
  document.querySelectorAll('.languages').forEach((group) => { group.hidden = false; });

  // GitHub Pages supplies the real account and repository; no invented username.
  const host = location.hostname;
  const repo = location.pathname.split('/').filter(Boolean)[0];
  if (/^[a-z0-9-]+\.github\.io$/i.test(host) && repo) {
    const owner = host.slice(0, -'.github.io'.length);
    document.querySelectorAll('.repo-link').forEach((link) => {
      link.href = `https://github.com/${owner}/${repo}`;
      link.hidden = false;
    });
  }
})();
