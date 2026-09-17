# Craft Web Portal

Official landing page and download center for **Craft**. Built with React 18, TypeScript, Vite, and Tailwind CSS.

Live URL: [https://craft.oguzhanumutlu.com](https://craft.oguzhanumutlu.com)

---

## Deployment via GitHub Actions (GitHub Pages)

The documentation portal is automatically built and deployed to **GitHub Pages** whenever changes are pushed to the `main` branch.

- **Workflow definition**: [`.github/workflows/deploy-docs.yml`](../.github/workflows/deploy-docs.yml)
- **Custom domain**: `craft.oguzhanumutlu.com` (configured via `docs/public/CNAME`)
- **Automated asset sync**: Root `docker-compose.yml` and `scripts/setup-vds.sh` are synchronized into `docs/public/` during the prebuild stage (`npm run prebuild`).
- **SPA 404 fallback**: `docs/dist/404.html` is automatically generated from `index.html` during the postbuild stage to ensure client-side routes and page refreshes work cleanly.

---

## Local Development & Testing

### Development Server
Run a hot-reloading development server on `http://localhost:3000`:
```bash
cd docs
npm run dev
```

### Production Build
Test the production build and verify static asset synchronization:
```bash
cd docs
npm run build
```

### Local Production Preview
Preview the production build locally on `http://localhost:4173`:
```bash
cd docs
npm run preview
```

---

## One-Time Repository & DNS Configuration

1. **GitHub Repository Settings**:
   - Go to **Settings > Pages**.
   - Under **Build and deployment > Source**, select **GitHub Actions**.

2. **Cloudflare DNS Configuration**:
   - Add or verify a `CNAME` record:
     - **Type**: `CNAME`
     - **Name**: `craft`
     - **Target**: `OguzhanUmutlu.github.io`
     - **Proxy status**: DNS Only (or Proxied if using Cloudflare WAF/caching).
