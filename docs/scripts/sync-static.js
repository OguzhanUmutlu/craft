import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const rootDir = path.resolve(__dirname, '../..');
const docsDir = path.resolve(__dirname, '..');
const publicDir = path.join(docsDir, 'public');

if (!fs.existsSync(publicDir)) {
  fs.mkdirSync(publicDir, { recursive: true });
}

// 1. Synchronize docker-compose.yml from root
const rootDockerCompose = path.join(rootDir, 'docker-compose.yml');
const publicDockerCompose = path.join(publicDir, 'docker-compose.yml');
if (fs.existsSync(rootDockerCompose)) {
  fs.copyFileSync(rootDockerCompose, publicDockerCompose);
  console.log('[OK] Synced docker-compose.yml to docs/public/');
} else {
  console.warn('[WARN] Root docker-compose.yml not found at:', rootDockerCompose);
}

// 2. Synchronize setup-vds.sh from scripts/
const rootSetupVds = path.join(rootDir, 'scripts/setup-vds.sh');
const publicSetupVds = path.join(publicDir, 'setup-vds.sh');
if (fs.existsSync(rootSetupVds)) {
  fs.copyFileSync(rootSetupVds, publicSetupVds);
  try {
    fs.chmodSync(publicSetupVds, 0o755);
  } catch {
    // Chmod may not apply on Windows, which is expected
  }
  console.log('[OK] Synced setup-vds.sh to docs/public/');
} else {
  console.warn('[WARN] scripts/setup-vds.sh not found at:', rootSetupVds);
}

// 3. Ensure CNAME file exists for custom domain
const cnamePath = path.join(publicDir, 'CNAME');
if (!fs.existsSync(cnamePath)) {
  fs.writeFileSync(cnamePath, 'craft.oguzhanumutlu.com\n', 'utf8');
  console.log('[OK] Created CNAME file in docs/public/');
}

// 4. Ensure .nojekyll file exists to avoid Jekyll filtering
const nojekyllPath = path.join(publicDir, '.nojekyll');
if (!fs.existsSync(nojekyllPath)) {
  fs.writeFileSync(nojekyllPath, '# Disable Jekyll for GitHub Pages\n', 'utf8');
  console.log('[OK] Created .nojekyll in docs/public/');
}
