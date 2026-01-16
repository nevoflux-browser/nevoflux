const fs = require("fs");
const path = require("path");
const Handlebars = require("handlebars");

const SCHEMA_PATH = path.join(__dirname, "nevoflux-api.json");
const OUTPUT_DIR = path.join(__dirname, "generated");
const TEMPLATES_DIR = path.join(__dirname, "templates");

// Handlebars helpers
Handlebars.registerHelper("mapType", function(field) {
  if (!field) return "void";
  if (field.$ref) {
    return field.$ref.split("/").pop();
  }
  if (field.type === "array") {
    return `${Handlebars.helpers.mapType(field.items)}[]`;
  }
  const typeMap = {
    string: "string",
    number: "number",
    integer: "number",
    boolean: "boolean",
    object: "Record<string, any>",
  };
  return typeMap[field.type] || "any";
});

Handlebars.registerHelper("isRequired", function(key, required) {
  return required && required.includes(key);
});

async function generate() {
  console.log("Generating API types from schema...\n");

  const schema = JSON.parse(fs.readFileSync(SCHEMA_PATH, "utf-8"));

  // Ensure output directory exists
  if (!fs.existsSync(OUTPUT_DIR)) {
    fs.mkdirSync(OUTPUT_DIR, { recursive: true });
  }

  // Generate TypeScript types
  generateTypeScript(schema);

  // Generate LLM tools JSON
  generateLLMTools(schema);

  // Generate version file
  generateVersion(schema);

  console.log("\n✅ Generation complete!");
}

function generateTypeScript(schema) {
  const templatePath = path.join(TEMPLATES_DIR, "typescript.hbs");

  if (!fs.existsSync(templatePath)) {
    console.log("  ⚠️  TypeScript template not found, creating minimal output");

    // Generate minimal TypeScript without template
    let output = `/**
 * NevoFlux Browser API Types
 * Version: ${schema.version}
 * Generated: ${new Date().toISOString()}
 */

`;

    // Generate definitions
    for (const [name, def] of Object.entries(schema.definitions)) {
      output += `export interface ${name} {\n`;
      if (def.properties) {
        for (const [propName, prop] of Object.entries(def.properties)) {
          const optional = !def.required?.includes(propName) ? "?" : "";
          const type = mapTypeSimple(prop);
          output += `  ${propName}${optional}: ${type};\n`;
        }
      }
      output += `}\n\n`;
    }

    // Generate API interface
    output += `export interface NevofluxAPI {\n`;
    for (const [nsName, ns] of Object.entries(schema.namespaces)) {
      output += `  ${nsName}: {\n`;
      for (const [methodName, method] of Object.entries(ns.methods)) {
        const params = Object.entries(method.params || {})
          .map(([k, v]) => `${k}${v.optional ? "?" : ""}: ${mapTypeSimple(v)}`)
          .join(", ");
        const returnType = mapTypeSimple(method.returns);
        output += `    ${methodName}(${params}): Promise<${returnType}>;\n`;
      }
      output += `  };\n`;
    }
    output += `}\n`;

    fs.writeFileSync(path.join(OUTPUT_DIR, "types.ts"), output);
    console.log("  → generated/types.ts");
    return;
  }

  const template = Handlebars.compile(fs.readFileSync(templatePath, "utf-8"));
  const output = template({
    version: schema.version,
    generatedAt: new Date().toISOString(),
    namespaces: schema.namespaces,
    definitions: schema.definitions,
  });

  fs.writeFileSync(path.join(OUTPUT_DIR, "types.ts"), output);
  console.log("  → generated/types.ts");
}

function generateLLMTools(schema) {
  const tools = [];
  const toolsByMode = { chat: [], agent: [], browser_use: [] };

  for (const [nsName, ns] of Object.entries(schema.namespaces)) {
    for (const [methodName, method] of Object.entries(ns.methods)) {
      const tool = {
        name: `browser_use.${nsName}.${methodName}`,
        description: method.description,
        mode: ns.mode,
        input_schema: {
          type: "object",
          properties: method.params,
          required: Object.entries(method.params)
            .filter(([_, v]) => !v.optional)
            .map(([k]) => k),
        },
      };
      tools.push(tool);

      // Add to mode-specific list and inherited modes
      if (ns.mode === "chat") {
        toolsByMode.chat.push(tool.name);
        toolsByMode.agent.push(tool.name);
        toolsByMode.browser_use.push(tool.name);
      } else if (ns.mode === "agent") {
        toolsByMode.agent.push(tool.name);
        toolsByMode.browser_use.push(tool.name);
      } else if (ns.mode === "browser_use") {
        toolsByMode.browser_use.push(tool.name);
      }
    }
  }

  const output = { version: schema.version, tools, toolsByMode };
  fs.writeFileSync(
    path.join(OUTPUT_DIR, "llm-tools.json"),
    JSON.stringify(output, null, 2)
  );
  console.log("  → generated/llm-tools.json");
}

function generateVersion(schema) {
  const crypto = require("crypto");
  const checksum = crypto
    .createHash("md5")
    .update(JSON.stringify(schema))
    .digest("hex")
    .slice(0, 8);

  const versionInfo = {
    version: schema.version,
    generatedAt: new Date().toISOString(),
    checksum,
  };

  fs.writeFileSync(
    path.join(OUTPUT_DIR, "schema-version.json"),
    JSON.stringify(versionInfo, null, 2)
  );
  console.log("  → generated/schema-version.json");
}

function mapTypeSimple(field) {
  if (!field) return "void";
  if (field.$ref) return field.$ref.split("/").pop();
  if (field.type === "array") return `${mapTypeSimple(field.items)}[]`;
  const typeMap = { string: "string", number: "number", integer: "number", boolean: "boolean", object: "Record<string, any>" };
  return typeMap[field.type] || "any";
}

generate().catch(console.error);
