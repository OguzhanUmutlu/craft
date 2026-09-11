# Craft Web Portal

Official landing page and download center for **Craft**. Built with React 18, TypeScript, Vite, and Tailwind CSS.

---

## Deployment to Cloudflare Workers

Craft Web is pre-configured for **Cloudflare Workers Static Assets** (`assets = { directory: "./dist" }`) with custom domain mapping for `craft.oguzhanumutlu.net`.

### Single-Command Automated Deployment
```bash
cd docs
npm run deploy
```
*(This automatically runs `vite build` to compile the static bundle and runs `wrangler deploy` to publish it to Cloudflare edge and bind `craft.oguzhanumutlu.net`)*.

---

### First-Time Setup (Login)
If you haven't logged in to Wrangler on this machine yet:
```bash
npx wrangler login
```

For detailed instructions on custom domains, previewing, and SSL setup, refer to the private deployment guide in `tools/cloudflare_worker_guide.md`.
