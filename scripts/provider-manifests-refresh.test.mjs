// Check change detection and header rewriting of the manifest refresh script without running cargo.
// 在不运行 cargo 的情况下校验清单刷新脚本的变更检测与头部改写。
import assert from 'node:assert/strict';
import test from 'node:test';
import { bumpManifestHeader, planRefresh, sha256 } from './provider-manifests-refresh.mjs';

// manifestText builds a minimal hand-formatted manifest body.
// manifestText 构造最小的手工排版清单正文。
const manifestText = (revision, extra = '') =>
  `{\n  "provider_id": "p",\n  "revision": ${revision},\n  "published_at": "2026-01-01T00:00:00Z",\n  "display": {${extra}}\n}\n`;

test('unchanged manifests need no index rebuild', () => {
  const bytes = Buffer.from(manifestText(3));
  const index = { index_revision: 7, providers: { p: { revision: 3, sha256: sha256(bytes) } } };
  const plan = planRefresh(index, { p: { bytes, revision: 3 } });
  assert.equal(plan.rebuildIndex, false);
  assert.deepEqual(plan.bumps, []);
});

test('an edited manifest without a raised revision is bumped past the pinned revision', () => {
  const pinned = Buffer.from(manifestText(3));
  const edited = Buffer.from(manifestText(3, '"name": "x"'));
  const index = { index_revision: 7, providers: { p: { revision: 3, sha256: sha256(pinned) } } };
  const plan = planRefresh(index, { p: { bytes: edited, revision: 3 } });
  assert.equal(plan.rebuildIndex, true);
  assert.equal(plan.indexRevision, 8);
  assert.deepEqual(plan.bumps, [{ providerId: 'p', revision: 4 }]);
});

test('an author-raised revision is kept and new or removed providers rebuild the index', () => {
  const pinned = Buffer.from(manifestText(3));
  const raised = Buffer.from(manifestText(5, '"name": "x"'));
  const index = {
    index_revision: 1,
    providers: { p: { revision: 3, sha256: sha256(pinned) }, gone: { revision: 1, sha256: 'x' } },
  };
  const plan = planRefresh(index, { p: { bytes: raised, revision: 5 }, fresh: { bytes: pinned, revision: 1 } });
  assert.deepEqual(plan.bumps, []);
  assert.deepEqual(plan.changed.sort(), ['fresh', 'p']);
  assert.deepEqual(plan.removed, ['gone']);
});

test('a core list edit rebuilds the index and bumps only a revision that was not raised', () => {
  const bytes = Buffer.from(manifestText(3));
  const pinnedCore = Buffer.from(manifestText(2));
  const index = {
    index_revision: 7,
    providers: { p: { revision: 3, sha256: sha256(bytes) } },
    core_models: { revision: 2, sha256: sha256(pinnedCore) },
  };
  const manifests = { p: { bytes, revision: 3 } };
  const unchanged = planRefresh(index, manifests, { bytes: pinnedCore, revision: 2 });
  assert.equal(unchanged.rebuildIndex, false);
  const edited = planRefresh(index, manifests, { bytes: Buffer.from(manifestText(2, '"x": 1')), revision: 2 });
  assert.equal(edited.rebuildIndex, true);
  assert.equal(edited.coreBump, 3);
  const raised = planRefresh(index, manifests, { bytes: Buffer.from(manifestText(4)), revision: 4 });
  assert.equal(raised.coreBump, null);
  const legacy = planRefresh({ index_revision: 7, providers: index.providers }, manifests, { bytes: pinnedCore, revision: 1 });
  assert.equal(legacy.rebuildIndex, true);
  assert.equal(legacy.coreBump, null);
});

test('header rewrite changes only the top-level revision and publication time', () => {
  const text = manifestText(3, '"nested": {"revision": 1}');
  const rewritten = bumpManifestHeader(text, 4, '2026-10-19T00:00:00Z');
  const parsed = JSON.parse(rewritten);
  assert.equal(parsed.revision, 4);
  assert.equal(parsed.published_at, '2026-10-19T00:00:00Z');
  assert.deepEqual(parsed.display, JSON.parse(text).display);
  assert.throws(() => bumpManifestHeader('{}', 1, 'x'));
});
