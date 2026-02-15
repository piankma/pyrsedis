#!/usr/bin/env python3
"""Migrate client.rs methods from two-pass to single-pass exec_raw."""
import re

with open("src/client.rs", "r") as f:
    content = f.read()

# Pattern 1: Simple commands with literal array args
# let result = py.detach(|| {
#     runtime::block_on(self.router.execute(&["CMD", ...]))
# }).map_err(|e| -> PyErr { e.into() })?;
# self.to_python(py, result)
pattern1 = re.compile(
    r'let result = py\.detach\(\|\| \{\s*'
    r'runtime::block_on\(self\.router\.execute\((&\[.*?\])\)\)\s*'
    r'\}\)\.map_err\(\|e\| -> PyErr \{ e\.into\(\) \}\)\?;\s*'
    r'self\.to_python\(py, result\)',
    re.DOTALL
)

count1 = [0]
def replace1(m):
    count1[0] += 1
    args = m.group(1)
    return f'self.exec_raw(py, {args})?'

content = pattern1.sub(replace1, content)
print(f"Pattern 1 (literal arrays): {count1[0]}")

# Pattern 2: Commands with variable args (&cmd, &refs, etc.)
pattern2 = re.compile(
    r'let result = py\.detach\(\|\| \{\s*'
    r'runtime::block_on\(self\.router\.execute\((&\w+)\)\)\s*'
    r'\}\)\.map_err\(\|e\| -> PyErr \{ e\.into\(\) \}\)\?;\s*'
    r'self\.to_python\(py, result\)',
    re.DOTALL
)

count2 = [0]
def replace2(m):
    count2[0] += 1
    args = m.group(1)
    return f'self.exec_raw(py, {args})?'

content = pattern2.sub(replace2, content)
print(f"Pattern 2 (variable args): {count2[0]}")

with open("src/client.rs", "w") as f:
    f.write(content)

print(f"Total replaced: {count1[0] + count2[0]}")
