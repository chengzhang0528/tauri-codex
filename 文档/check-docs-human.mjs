import fs from "node:fs";
import path from "node:path";

function markdownTargets(text) {
  return [...text.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)].map((match) => match[1]);
}

function isInside(directory, target) {
  const relative = path.relative(directory, target);
  return relative !== "" && !relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative);
}

export function validateHumanDocumentation({ root, files, resolveReference }) {
  const errors = [];
  const humanDocsRoot = path.join(root, "人类-文档");
  const rootReadme = path.join(root, "README.md");
  const humanDocsReadme = path.join(humanDocsRoot, "README.md");
  const packagePath = path.join(root, "package.json");
  const rootReadmeExists = fs.existsSync(rootReadme);
  const humanDocsReadmeExists = fs.existsSync(humanDocsReadme);
  const humanMarkdown = [
    ...(rootReadmeExists ? [rootReadme] : []),
    ...files.filter((file) => file.startsWith(`${humanDocsRoot}${path.sep}`)),
  ];

  if (!rootReadmeExists) {
    errors.push("README.md: missing root human documentation entry");
  }
  if (!humanDocsReadmeExists) {
    errors.push("人类-文档/README.md: missing human documentation entry");
  }

  let packageScripts = new Set();
  try {
    packageScripts = new Set(Object.keys(JSON.parse(fs.readFileSync(packagePath, "utf8")).scripts ?? {}));
  } catch (error) {
    errors.push(`human documentation package script validation failed: ${error.message}`);
  }

  for (const file of humanMarkdown) {
    const label = path.relative(root, file).split(path.sep).join("/");
    const text = fs.readFileSync(file, "utf8");
    for (const rawTarget of markdownTargets(text)) {
      const target = resolveReference(file, rawTarget);
      if (target && !fs.existsSync(target)) {
        errors.push(`${label}: broken human documentation link ${rawTarget}`);
      }
    }
    for (const match of text.matchAll(/\bnpm run ([A-Za-z0-9:_-]+)/g)) {
      if (!packageScripts.has(match[1])) {
        errors.push(`${label}: unknown package script ${match[1]}`);
      }
    }

    if (file.startsWith(`${humanDocsRoot}${path.sep}`) && path.basename(file) !== "README.md") {
      const stem = path.basename(file, path.extname(file));
      const h1 = text.match(/^#\s+(.+?)\s*$/m)?.[1];
      if (!h1) {
        errors.push(`${label}: missing H1 title for filename comparison`);
      } else if (h1 !== stem) {
        errors.push(`${label}: filename does not match H1 "${h1}"`);
      }
    }
  }

  if (rootReadmeExists) {
    const targets = markdownTargets(fs.readFileSync(rootReadme, "utf8"))
      .map((target) => resolveReference(rootReadme, target));
    if (!targets.some((target) => target === humanDocsReadme)) {
      errors.push("README.md: root human entry must link to 人类-文档/README.md");
    }
  }

  if (humanDocsReadmeExists) {
    const targets = markdownTargets(fs.readFileSync(humanDocsReadme, "utf8"))
      .map((target) => resolveReference(humanDocsReadme, target))
      .filter(Boolean);
    const hasTaskPage = targets.some((target) => {
      return target !== humanDocsReadme
        && isInside(humanDocsRoot, target)
        && /\.mdx?$/i.test(target)
        && fs.existsSync(target);
    });
    if (!hasTaskPage) {
      errors.push("人类-文档/README.md: human entry must link to at least one task page");
    }
  }

  return errors;
}
