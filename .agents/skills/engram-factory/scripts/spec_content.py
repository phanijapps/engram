"""Derive structural readiness from a single human-readable spec source."""
import re

TEMPLATE_MARKERS = ('<Brief LED dispositions, consequential decisions, source evidence and assumptions.>', '<In scope, exclusions with authority, and affected interfaces or compatibility.>', '<Who needs this, what fails today, and what success means.>', '<action>', '<actor>', '<failure or boundary condition>', '<journey>', '<normal condition>', '<observable outcome>', '<observable result>', '<response>', '<rule IDs or justified N/A>', '<source and revision>', '<trigger>')

AUTHOR_FIELDS = {'schema_version', 'spec_format', 'size_tier', 'scope', 'required_reviewers', 'mode'}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def table(body, heading, columns):
    matches = list(re.finditer(r'^## ' + re.escape(heading) + r'\s*$', body, re.M))
    require(len(matches) == 1, 'Spec needs exactly one ' + heading + ' section')
    block = re.split(r'^## ', body[matches[0].end():], maxsplit=1, flags=re.M)[0]
    lines = [line.strip() for line in block.splitlines() if line.strip().startswith('|')]
    def cells(line):
        return [v.strip().replace(r'\|', '|') for v in re.split(r'(?<!\\)\|', line.strip('|'))]
    require(len(lines) >= 3 and cells(lines[0]) == columns, 'Invalid or empty ' + heading + ' table')
    require(len(cells(lines[1])) == len(columns) and all(re.fullmatch(r':?-{3,}:?', v) for v in cells(lines[1])), 'Invalid table separator: ' + heading)
    rows = []
    for line in lines[2:]:
        values = cells(line)
        require(len(values) == len(columns), 'Wrong table width: ' + heading)
        rows.append(dict(zip(columns, values)))
    return rows


def keyed(rows, pattern, name):
    result = {}
    for row in rows:
        key = row['ID']
        require(bool(re.fullmatch(pattern, key)) and key not in result, 'Invalid or duplicate ' + name + ': ' + key)
        require(all(row.values()), 'Empty ' + name + ' cell: ' + key)
        result[key] = row
    return result


def ids(value, prefix):
    parts = [p for p in re.split(r'[\s,]+', value) if p]
    require(parts and all(re.fullmatch(prefix + r'-\d{3}', p) for p in parts), 'Invalid ' + prefix + ' references')
    require(len(parts) == len(set(parts)), 'Duplicate ' + prefix + ' references')
    return parts


def derive(fields, body):
    require(set(fields) <= AUTHOR_FIELDS, 'Compact specs must not author machine metadata or readiness')
    require(type(fields.get('schema_version')) is int and fields['schema_version'] == 2 and fields.get('spec_format') == 'compact-v1', 'Invalid compact spec version')
    require(fields.get('size_tier') in {'S', 'M', 'L'}, 'Spec needs size_tier S/M/L')
    require(fields.get('scope') in {'single', 'multi-surface'}, 'Spec needs explicit scope')
    require(not re.search(r'\b(?:TBD|TODO|FIXME)\b|\[NEEDS CLARIFICATION', body), 'Unresolved spec content')
    require(not any(marker in body for marker in TEMPLATE_MARKERS), 'Unresolved template marker')
    for heading in ('Outcome', 'Boundaries', 'Decisions and evidence'):
        match = re.search(r'^## ' + heading + r'\s*\n(.*?)(?=^## |\Z)', body, re.M | re.S)
        require(match and len(match[1].strip()) >= 12, 'Missing substantive ' + heading)
    req = keyed(table(body, 'Requirements', ['ID', 'Behavior', 'Rules']), r'REQ-\d{3}', 'requirement')
    acceptance = keyed(table(body, 'Acceptance', ['ID', 'Requirement', 'Given', 'When', 'Then']), r'AC-\d{3}', 'acceptance')
    scenarios = keyed(table(body, 'Scenarios', ['ID', 'Priority', 'Actor', 'Journey', 'Outcome', 'Requirements', 'Acceptance', 'Evidence']), r'U-\d+', 'scenario')
    for ac, row in acceptance.items():
        require(row['Requirement'] in req, 'Acceptance references unknown requirement: ' + ac)
    require({r['Requirement'] for r in acceptance.values()} == set(req), 'Every requirement needs acceptance')
    rules = set()
    for row in req.values():
        value = row['Rules']
        if value.startswith('N/A:'):
            require(len(value[4:].strip()) >= 8, 'Rule N/A needs a scope reason')
        else:
            entries = re.split(r'[\s,]+', value)
            require(all(re.fullmatch(r'(?:[A-Z]{2,}-\d{3}|RULE-\d+)', v) for v in entries), 'Unknown or invalid rule applicability')
            rules.update(entries)
    covered = set()
    for sid, row in scenarios.items():
        requirements, acs = ids(row['Requirements'], 'REQ'), ids(row['Acceptance'], 'AC')
        require(set(requirements) <= set(req) and set(acs) <= set(acceptance), 'Scenario references unknown IDs: ' + sid)
        require({acceptance[a]['Requirement'] for a in acs} == set(requirements), 'Scenario requirement/acceptance mismatch: ' + sid)
        require(row['Priority'] in {'P1', 'P2', 'P3'}, 'Invalid scenario priority')
        require(all(row[key].strip() for key in ('Actor', 'Journey', 'Outcome', 'Evidence')), 'Scenario lacks actor, journey, outcome or evidence')
        covered.update(acs)
    require(covered == set(acceptance), 'Every acceptance criterion needs a scenario')
    if fields['scope'] == 'multi-surface':
        _surfaces(body, scenarios, req, acceptance)
    elif re.search(r'^## Surfaces\s*$', body, re.M):
        raise ValueError('Surfaces require multi-surface scope')
    return {'constitution_rules_touched': sorted(rules)}


def _surfaces(body, scenarios, req, acceptance):
    columns = ['ID', 'Capability', 'Classification', 'Authority', 'Entry', 'Navigation', 'Scenario', 'Requirements',
               'Acceptance', 'Normal', 'Empty', 'Loading', 'Error', 'Unavailable', 'Evidence']
    rows = keyed(table(body, 'Surfaces', columns), r'INV-\d+', 'surface')
    for key, row in rows.items():
        require(row['Capability'].strip() and row['Evidence'].strip(), 'Surface needs capability and evidence')
        classification = row['Classification']
        require(classification in {'in-scope', 'support', 'infrastructure', 'conditional', 'out-of-scope'}, 'Unknown surface classification')
        if classification in {'conditional', 'out-of-scope'}:
            require(row['Authority'].strip() and row['Authority'] != '-' and not row['Authority'].startswith(('unknown', 'N/A')), 'Excluded/conditional surface needs owner authority')
        if classification != 'in-scope':
            continue
        require(row['Scenario'] in scenarios, 'Surface references unknown scenario')
        acs, requirements = ids(row['Acceptance'], 'AC'), ids(row['Requirements'], 'REQ')
        require(set(acs) <= set(acceptance) and set(requirements) <= set(req), 'Surface references unknown IDs')
        require({acceptance[a]['Requirement'] for a in acs} == set(requirements), 'Surface requirement/acceptance mismatch')
        scenario = scenarios[row['Scenario']]
        require(set(acs) <= set(ids(scenario['Acceptance'], 'AC'))
                and set(requirements) <= set(ids(scenario['Requirements'], 'REQ')), 'Surface/scenario coverage mismatch')
        for state in ('Normal', 'Empty', 'Loading', 'Error', 'Unavailable'):
            value = row[state]
            if value.startswith('N/A:'):
                require(len(value[4:].strip()) >= 8, 'State N/A needs a reason')
            else:
                require(set(ids(value, 'AC')) <= set(acs), 'State acceptance missing from surface coverage')
        require(all(row[key].strip() and row[key] != '-' for key in ('Entry', 'Navigation')), 'Surface needs entry and navigation')
    require(any(row['Classification'] == 'in-scope' for row in rows.values()), 'Multi-surface scope needs in-scope scenario coverage')
    conditions = table(body, 'Review conditions', ['Condition', 'Evidence or protocol'])
    expected = {'Viewports and browser', 'Themes', 'Accessibility', 'Visual judgment', 'Reference evidence'}
    seen = set()
    for row in conditions:
        key = row['Condition']
        require(key in expected and key not in seen and len(row['Evidence or protocol'].strip()) >= 8, 'Invalid review condition')
        seen.add(key)
    require(seen == expected, 'Missing review condition')
