import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const expectedWorkflows = ['ci.yml', 'release-production.yml', 'runtime-updates.yml'];
const forbiddenWindowsSigning = /WINDOWS_CODESIGN|PFX|THUMBPRINT|AUTHENTICODE|sign-windows-if-configured|sign-all-pe|Get-AuthenticodeSignature|signCommand/i;

function read(root, relativePath) {
  const path = join(root, relativePath);
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function jobBlocks(workflow) {
  const matches = [...workflow.matchAll(/^  ([a-zA-Z0-9_-]+):\r?\n([\s\S]*?)(?=^  [a-zA-Z0-9_-]+:\r?\n|(?![\s\S]))/gm)];
  return new Map(matches.map((match) => [match[1], match[2]]));
}

export function checkReleaseWorkflows(root) {
  const errors = [];
  const workflowDir = join(root, '.github/workflows');
  const workflows = readdirSync(workflowDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.ya?ml$/i.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  if (JSON.stringify(workflows) !== JSON.stringify(expectedWorkflows)) {
    errors.push(`active workflows must be exactly ${expectedWorkflows.join(', ')}`);
  }

  const release = read(root, '.github/workflows/release-production.yml');
  const runtime = read(root, '.github/workflows/runtime-updates.yml');
  const prepareWindows = read(root, 'scripts/release/Prepare-WindowsRelease.ps1');
  const activeWindowsSources = [
    release,
    read(root, '.github/workflows/ci.yml'),
    runtime,
    read(root, 'src-tauri/tauri.windows.conf.json'),
    read(root, 'scripts/release/Publish-DoodleRayDownloads.ps1'),
  ].join('\n');
  if (forbiddenWindowsSigning.test(activeWindowsSources)) errors.push('active release paths must not contain Windows Authenticode prerequisites');

  if (!/^\s*workflow_dispatch:/m.test(release)) errors.push('production release must use workflow_dispatch');
  if (!/source_sha:/m.test(release) || !/dry_run:/m.test(release)) errors.push('production dispatch must require source_sha and expose dry_run');
  if (!/group:\s*release-production/m.test(release) || !/cancel-in-progress:\s*false/m.test(release)) errors.push('production release concurrency contract is missing');
  if (!/^permissions:\r?\n\s+contents:\s+read/m.test(release)) errors.push('production workflow must default to contents: read');
  if (!/github\.ref[\s\S]{0,80}refs\/heads\/main/.test(release)
    || !/merge-base --is-ancestor/.test(release)
    || !/rev-parse (?:refs\/remotes\/)?origin\/main/.test(release)) {
    errors.push('production release must bind an exact source SHA to protected main');
  }
  if (!/release:check -- --published-version/.test(release)) errors.push('production release must enforce the published-version gate');
  if (!/TAURI_SIGNING_PRIVATE_KEY/.test(release) || !/\.sig/.test(release)) errors.push('Tauri updater signing and signatures must fail closed');
  if (!/\*\.nsis\.zip\.sig/.test(prepareWindows) || /\*\.exe\.sig/.test(prepareWindows)) errors.push('Windows release set must require the single Tauri NSIS updater signature');
  if (!/APPLE_CERTIFICATE/.test(release) || !/upload-app-store\.sh/.test(release)) errors.push('enabled App Store target must retain signing and upload gates');
  if (!/Production releases require both Windows and macAppStore targets/.test(release)) errors.push('production target dependencies must fail closed unless both targets are enabled');

  const windowsBuildCount = (release.match(/npx tauri build --bundles nsis/g) ?? []).length;
  if (windowsBuildCount !== 1) errors.push('production release must contain exactly one Windows Tauri build');
  const deployBlocks = [...jobBlocks(release)]
    .filter(([name]) => /deploy|publish|promote|upload/.test(name))
    .map(([, block]) => block)
    .join('\n');
  if (/npx tauri build|cargo build|npm run build/.test(deployBlocks)) errors.push('deploy jobs must not rebuild artifacts');
  if (!/actions\/download-artifact@/.test(deployBlocks)) errors.push('deploy jobs must download retained build artifacts');
  if (!/inputs\.dry_run\s*==\s*false/.test(deployBlocks)) errors.push('all external mutation jobs must be disabled in dry-run mode');
  if (!/different hashes.*hard fail/is.test(release) || !/same hashes.*no-op/is.test(release) || !/latest\.json.*last/is.test(release)) {
    errors.push('exact-byte idempotency and latest.json-last contracts must be explicit');
  }

  if (/\bdevelop\b/.test(runtime) || !/BASE_BRANCH:\s*main/.test(runtime) || !/--delete-branch/.test(runtime)) errors.push('runtime updater must target main with a short-lived branch and never develop');

  for (const path of [
    'src-tauri/sign-windows-if-configured.ps1',
    'scripts/sign-all-pe.ps1',
    'scripts/verify-signatures.ps1',
    'src-tauri/tauri.microsoftstore.conf.json',
    'scripts/build-store.ps1',
    'scripts/verify-store-installer.ps1',
  ]) {
    if (existsSync(join(root, path))) errors.push(`obsolete release path must be removed: ${path}`);
  }

  if (errors.length) throw new Error(`Workflow checks failed: ${errors.join('; ')}`);
  return expectedWorkflows;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    checkReleaseWorkflows(fileURLToPath(new URL('../../', import.meta.url)));
    console.log(`Release workflow checks passed: ${expectedWorkflows.map((name) => basename(name)).join(', ')}.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
