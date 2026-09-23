# Cloudflare Worker Deployment Guide: Craft Portal

This guide walks you through deploying the static **React + TSX + Vite + PostCSS** Craft portal onto **Cloudflare Workers** using the new **Cloudflare Workers Static Assets** feature.

---

## What You Are Deploying
- **Frontend**: A zero-latency, modern dark-mode landing page and download center for Craft.
- **Hosting**: Cloudflare's global edge network (300+ cities worldwide) with 0ms cold starts and automatic SSL.
- **Backend Connection**: The web app points download links and 1-line installation scripts (`curl | bash` and `irm | iex`) to your Go distribution server running on your VDS (`ssh saga` / `185.157.46.103:8080`).

---

## Prerequisites
1. **Node.js** (v18+ or v20+) and **npm** (already available on your machine).
2. A free or paid **[Cloudflare Account](https://dash.cloudflare.com/sign-up)**.

---

## Step-by-Step Deployment

### Automated Single-Command Deployment (Recommended)
You can deploy everything in one step using the automated script:
```bash
./scripts/deploy_docs.sh
# or: make deploy-docs
```

---

### Manual Deployment Steps

#### Step 1: Authenticate with Cloudflare
Open your terminal and log in to your Cloudflare account via Wrangler (Cloudflare's official CLI):

```bash
cd docs
npx wrangler login
```

A browser window will open asking you to authorize Wrangler. Click **"Allow"**.

To verify you are logged in:
```bash
npx wrangler whoami
```

---

#### Step 2: Build the Production Static Bundle
Compile the React TypeScript app and Tailwind styles into the `./dist` folder:

```bash
cd docs
npm run build
```

You should see:
```text
[OK] built in 2.4s
dist/index.html
dist/assets/index-*.css
dist/assets/index-*.js
```

---

### Step 3: (Optional) Preview Locally on Cloudflare's Edge Runtime
Before deploying live, you can test how the worker serves your static assets locally:

```bash
npx wrangler dev
```

Visit `http://localhost:8787` in your browser. Press `q` to exit when done.

---

### Step 4: Deploy to Cloudflare Workers
Deploy the built site to Cloudflare with a single command:

```bash
npx wrangler deploy
```

Wrangler will upload your assets and print your live URL:
```text
Total Upload: xx.xx KiB / gzip: xx.xx KiB
Uploaded 100% (3/3 assets)
Deployed craft-web triggers (1.23 sec)
  https://craft-web.<your-subdomain>.workers.dev
```

You can now open that URL in any browser or share it with users!

---

## Step 5: Adding a Custom Domain (e.g. `craft.yourdomain.com`)

If you manage your domain on Cloudflare DNS, you can attach a custom domain in seconds:

### Method A: Via Cloudflare Dashboard
1. Go to **[Cloudflare Dashboard](https://dash.cloudflare.com/)**.
2. In the left sidebar, select **Workers & Pages**.
3. Click on the **`craft-web`** worker.
4. Navigate to **Settings** &rarr; **Domains & Routes**.
5. Click **"Add"** &rarr; **"Custom Domain"**.
6. Enter `craft.yourdomain.com` (or your preferred domain) and click **"Add Custom Domain"**.
7. Cloudflare will automatically provision a free SSL/TLS certificate within a few minutes.

### Method B: Via `docs/wrangler.jsonc` (Already Configured)
The route is already configured directly in `docs/wrangler.jsonc`:

```jsonc
{
  "$schema": "node_modules/wrangler/config-schema.json",
  "name": "craft-docs",
  "compatibility_date": "2026-09-01",
  "compatibility_flags": ["nodejs_compat"],
  "assets": {
    "directory": "./dist",
    "html_handling": "auto-trailing-slash",
    "not_found_handling": "single-page-application"
  },
  "routes": [
    {
      "pattern": "craft.oguzhanumutlu.com",
      "custom_domain": true
    }
  ]
}
```
Simply running `npm run deploy` inside `docs/` (or `./scripts/deploy_docs.sh`) will build and deploy it with this domain attached.

---

## Step 6: Connecting with your VDS Distribution Server

Your Go distribution server is hosted on your VDS (`ssh saga`):
- Direct IP endpoint: `http://185.157.46.103:8080`

### Optional: Recommended SSL / Reverse Proxy for VDS
If your web frontend is on `https://craft.yourdomain.com`, browsers may block downloads if the backend is plain `http://` (mixed content).

To give your VDS server a clean HTTPS domain (e.g. `https://dl.yourdomain.com`):

#### Using Caddy on the VDS (1-minute setup):
SSH into the VDS:
```bash
ssh saga
```
Install Caddy (if not installed):
```bash
apt install -y caddy
```
Edit `/etc/caddy/Caddyfile`:
```caddy
dl.yourdomain.com {
    reverse_proxy 127.0.0.1:8080
}
```
Reload Caddy:
```bash
systemctl reload caddy
```
Now `https://dl.yourdomain.com/install.sh` and `https://dl.yourdomain.com/api/v1/version` will have automatic HTTPS!

Once you have your custom domain, update `VDS_HOST` in `docs/src/App.tsx`:
```tsx
const VDS_HOST = "dl.yourdomain.com";
const VDS_BASE_URL = `https://${VDS_HOST}`;
```
And re-deploy:
```bash
./scripts/deploy_docs.sh
```

---

## Maintenance & Cheat Sheet

| Task | Command |
| :--- | :--- |
| **Re-deploy after code changes** | `npm run build && npx wrangler deploy` |
| **Inspect real-time live traffic** | `npx wrangler tail` |
| **List deployments & versions** | `npx wrangler deployments list` |
| **Rollback to previous version** | `npx wrangler rollback` |
| **Update VDS distribution server** | `./tools/deploy_saga.sh saga` |
| **Check VDS service status** | `ssh saga systemctl status craft-server` |

---

## Security Note
The entire `./tools/` directory is registered in `.gitignore` to guarantee that your deployment scripts, server binaries, and Cloudflare configurations remain private to your workstation.
