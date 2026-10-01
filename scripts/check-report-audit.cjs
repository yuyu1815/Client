#!/usr/bin/env node
'use strict';
const fs = require('node:fs');
const path = require('node:path');

const repo = path.resolve(__dirname, '..');
const audit = path.join(repo, 'docs/report-audit');
const ledger = JSON.parse(fs.readFileSync(path.join(audit, 'coverage-ledger.json'), 'utf8'));
const errors = [];
const batchIds = [];
function recordsFrom(batch, n) {
  if (n === 3) return batch;
  if (n === 7) return [...(batch.confirmed_records || []), ...(batch.candidate_records || [])];
  if (n === 9) return (batch.objects || []).flatMap(o => (o.records || []).map(r => ({ ...r, object_id: r.object_id || o.target })));
  return batch.records || [];
}
for (let n = 1; n <= 9; n++) {
  const file = path.join(audit, `batch-${String(n).padStart(2, '0')}.json`);
  const batch = JSON.parse(fs.readFileSync(file, 'utf8'));
  const rows = recordsFrom(batch, n);
  for (const row of rows) {
    if (!row.id) errors.push(`${path.basename(file)}: record without id`);
    batchIds.push(row.id);
  }
}
const ledgerIds = ledger.records.map(row => row.id);
function duplicateIds(ids) {
  const seen = new Set(), duplicates = new Set();
  for (const id of ids) seen.has(id) ? duplicates.add(id) : seen.add(id);
  return [...duplicates].sort();
}
const batchDuplicates = duplicateIds(batchIds);
const ledgerDuplicates = duplicateIds(ledgerIds);
const ledgerSet = new Set(ledgerIds), batchSet = new Set(batchIds);
const missingInLedger = [...batchSet].filter(id => !ledgerSet.has(id)).sort();
const ledgerNotInBatches = [...ledgerSet].filter(id => !batchSet.has(id)).sort();
if (batchIds.length !== ledger.scope.recordsExpected) errors.push(`batch record count ${batchIds.length}, expected ${ledger.scope.recordsExpected}`);
if (ledger.records.length !== ledger.scope.recordsExpected) errors.push(`ledger row count ${ledger.records.length}, expected ${ledger.scope.recordsExpected}`);
if (batchDuplicates.length) errors.push(`duplicate batch IDs: ${batchDuplicates.join(', ')}`);
if (ledgerDuplicates.length) errors.push(`duplicate ledger IDs: ${ledgerDuplicates.join(', ')}`);
if (missingInLedger.length) errors.push(`IDs missing from ledger: ${missingInLedger.join(', ')}`);
if (ledgerNotInBatches.length) errors.push(`ledger IDs absent from batches: ${ledgerNotInBatches.join(', ')}`);
for (const row of ledger.records) {
  for (const key of ['batch', 'objectId', 'reportClassification', 'causeConfidence', 'implementationStatus', 'verificationStatus']) {
    if (row[key] == null || row[key] === '') errors.push(`${row.id}: missing ${key}`);
  }
}
const ledgerObjects = new Set(ledger.records.map(r => r.objectId));
if (ledgerObjects.size > ledger.scope.objectsExpected) errors.push(`ledger object IDs ${ledgerObjects.size} exceeds expected ${ledger.scope.objectsExpected}`);

// External report data is optional; pass its root as argv[2] or REPORT_ROOT.
const reportRoot = process.argv[2] || process.env.REPORT_ROOT;
const objectIndexFile = reportRoot && path.join(reportRoot, 'object_index.json');
const coverageFile = reportRoot && path.join(reportRoot, 'coverage.json');
let externalChecked = false;
if (objectIndexFile && coverageFile && fs.existsSync(objectIndexFile) && fs.existsSync(coverageFile)) {
  externalChecked = true;
  const index = JSON.parse(fs.readFileSync(objectIndexFile, 'utf8'));
  const coverage = JSON.parse(fs.readFileSync(coverageFile, 'utf8'));
  const indexIds = index.flatMap(obj => obj.record_ids || []);
  const indexedObjectById = new Map(index.flatMap(obj => (obj.record_ids || []).map(id => [id, obj.object_id])));
  const objectMismatches = ledger.records.filter(row => indexedObjectById.get(row.id) !== row.objectId).map(row => row.id);
  const indexDuplicates = duplicateIds(indexIds);
  const indexSet = new Set(indexIds);
  const indexMissing = [...batchSet].filter(id => !indexSet.has(id)).sort();
  const indexExtra = [...indexSet].filter(id => !batchSet.has(id)).sort();
  if (index.length !== ledger.scope.objectsExpected) errors.push(`object_index entries ${index.length}, expected ${ledger.scope.objectsExpected}`);
  if (indexIds.length !== ledger.scope.recordsExpected) errors.push(`object_index record references ${indexIds.length}, expected ${ledger.scope.recordsExpected}`);
  if (indexDuplicates.length) errors.push(`duplicate object_index record references: ${indexDuplicates.join(', ')}`);
  if (indexMissing.length) errors.push(`batch IDs missing from object_index: ${indexMissing.join(', ')}`);
  if (indexExtra.length) errors.push(`object_index IDs absent from batches: ${indexExtra.join(', ')}`);
  if (objectMismatches.length) errors.push(`ledger/object_index object mismatch: ${objectMismatches.join(', ')}`);
  if (coverage.objects !== ledger.scope.objectsExpected || coverage.total_records !== ledger.scope.recordsExpected)
    errors.push(`coverage counts objects=${coverage.objects}, records=${coverage.total_records}`);
  const shape = ledger.scope.shapeAuditSeparate;
  if (coverage.shape_blocks !== shape.blocks || coverage.shape_rows !== shape.states)
    errors.push(`shape totals differ: blocks=${coverage.shape_blocks}, states=${coverage.shape_rows}`);
}
console.log(`batches=${batchIds.length} unique=${batchSet.size} duplicates=${batchDuplicates.length}`);
console.log(`ledger=${ledger.records.length} unique=${ledgerSet.size} duplicates=${ledgerDuplicates.length} missing=${missingInLedger.length} extra=${ledgerNotInBatches.length}`);
console.log(`ledger_objects=${ledgerObjects.size} object_index=${externalChecked ? 'checked' : 'unavailable/skipped'} external=${externalChecked ? reportRoot : 'not required'}`);
console.log(`separate_shapes=${ledger.scope.shapeAuditSeparate.blocks} blocks/${ledger.scope.shapeAuditSeparate.states} states (not record IDs)`);
if (errors.length) { console.error(errors.map(e => `ERROR: ${e}`).join('\n')); process.exitCode = 1; }
else console.log('OK');
