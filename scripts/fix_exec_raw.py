#!/usr/bin/env python3
"""Fix exec_raw calls: remove trailing ? and re-add Router import."""
import re

with open("src/client.rs", "r") as f:
    content = f.read()

# Fix 1: Remove trailing `?` from `self.exec_raw(...)?\n    }` patterns
# The exec_raw method returns PyResult<Py<PyAny>> directly — no `?` needed
# when it's the last expression in a function returning PyResult<Py<PyAny>>.
count = [0]
def fix_trailing_q(m):
    count[0] += 1
    return m.group(0).replace(')?', ')')

# Match self.exec_raw(py, ...)?\n at end of function body
content = re.sub(r'self\.exec_raw\(py, [^)]+\)\?', 
    lambda m: m.group(0)[:-1],  # strip trailing ?
    content)

print(f"Fixed trailing ?: {content.count('self.exec_raw') - content.count('self.exec_raw(py')}")

# Fix 2: Re-add Router trait import
if 'use crate::router::Router;' not in content:
    content = content.replace(
        'use crate::router::standalone::StandaloneRouter;',
        'use crate::router::Router;\nuse crate::router::standalone::StandaloneRouter;'
    )
    print("Re-added Router import")

with open("src/client.rs", "w") as f:
    f.write(content)

print("Done")
