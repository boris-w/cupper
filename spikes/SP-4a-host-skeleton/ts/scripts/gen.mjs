// Generate TS types and zod validators from the committed JSON Schema.
// hey-api reads OpenAPI, so the schema's $defs become components.schemas.
import { readFileSync } from 'node:fs';
import { createClient } from '@hey-api/openapi-ts';

const schema = JSON.parse(readFileSync(new URL('../../protocol/schema/v1.json', import.meta.url), 'utf8'));

const rewrite = (v) => {
  if (Array.isArray(v)) return v.map(rewrite);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.entries(v).map(([k, x]) => [k, k === '$ref' ? x.replace('#/$defs/', '#/components/schemas/') : rewrite(x)]),
    );
  }
  return v;
};

const spec = {
  openapi: '3.1.0',
  info: { title: schema.title, version: String(schema['x-protocol-version']) },
  paths: {},
  components: { schemas: rewrite(schema.$defs) },
};

await createClient({
  input: { path: spec },
  output: { path: 'src/gen', importFileExtension: '.ts' },
  logs: { level: 'warn' },
  plugins: ['@hey-api/typescript', 'zod'],
});
