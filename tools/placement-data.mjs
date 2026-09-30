// Compact existing vanilla extraction; run from the repository root with node tools/placement-data.mjs [--check].
import fs from 'node:fs';
import assert from 'node:assert/strict';

const root = 'third_party/SteelMC/steel-registry/build_assets/';
const blocks = JSON.parse(fs.readFileSync(root + 'blocks.json', 'utf8')).blocks;
const items = JSON.parse(fs.readFileSync(root + 'items.json', 'utf8')).items;
const names = new Set(blocks.map(block => block.name));
const blockItems = {};
for (const item of items) {
    if (item.blockItem === undefined) continue;
    assert(names.has(item.blockItem), `missing block: ${item.blockItem}`);
    assert.equal(typeof item.class, 'string');
    blockItems[item.name] = { block: item.blockItem, class: item.class };
}
const replaceable = blocks.filter(block => {
    assert.equal(typeof block.behavior_properties.replaceable, 'boolean');
    return block.behavior_properties.replaceable;
}).map(block => block.name);
assert.equal(blockItems.oak_log.block, 'oak_log');
assert.equal(blockItems.redstone.block, 'redstone_wire');
assert.equal(blockItems.torch.class, 'StandingAndWallBlockItem');
assert(!('stick' in blockItems));
assert(replaceable.includes('water') && !replaceable.includes('oak_slab'));
const output = JSON.stringify({ block_items: blockItems, replaceable }, null, 2) + '\n';
const path = 'pomme-client/src/world/block/data/placement-26.2.json';
if (process.argv.includes('--check')) {
    assert.equal(fs.readFileSync(path, 'utf8'), output, 'regenerate placement data');
} else {
    fs.writeFileSync(path, output);
}
console.log(`${path}: ${Object.keys(blockItems).length} block items, ${replaceable.length} replaceable blocks`);
