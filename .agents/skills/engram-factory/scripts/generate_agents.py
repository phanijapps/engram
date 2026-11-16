#!/usr/bin/env python3
"""Generate host metadata and thin pointers from one factory role manifest."""
import sys
sys.dont_write_bytecode = True

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import stat

MARKER = 'engram-factory generated; bundle-sha256: '
HOSTS = ('claude', 'codex', 'copilot', 'pi')
PREFIX = '.agents/skills/engram-factory/'
NAME = r'engram-[a-z][a-z0-9]*(?:-[a-z0-9]+)*'
DISCIPLINES = {'code', 'security', 'platform', 'ux', 'ui', 'backend', 'data', 'operations'}


class GenerationError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise GenerationError(message)


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate manifest key: ' + key)
        result[key] = value
    return result


def _root(path):
    path = Path(path).absolute()
    require('..' not in path.parts, 'Root path must be canonical')
    for part in (path, *path.parents):
        require(not part.is_symlink(), 'Symlink root component is forbidden: ' + str(part))
    require(path.is_dir(), 'Missing root directory: ' + str(path))
    return path


def _path(root, relative, required=False):
    require(isinstance(relative, str) and relative and '\\' not in relative and ':' not in relative
            and not any(ord(c) < 32 or c in '`<>|' for c in relative),
            'Expected canonical relative path')
    part = Path(relative)
    require(not part.is_absolute() and part.as_posix() == relative
            and all(p not in {'', '.', '..'} for p in relative.split('/')), 'Noncanonical relative path: ' + relative)
    cursor = root
    for index, component in enumerate(part.parts):
        cursor = cursor / component
        require(not cursor.is_symlink(), 'Symlink path is forbidden: ' + str(cursor))
        if cursor.exists():
            require(cursor.is_file() if index == len(part.parts) - 1 else cursor.is_dir(),
                    'Wrong path type: ' + str(cursor))
    require(not required or cursor.is_file(), 'Missing source: ' + relative)
    return cursor


def _keys(value, expected, label):
    require(isinstance(value, dict) and set(value) == set(expected), 'Invalid ' + label + ' fields')


def _text(value, label):
    require(isinstance(value, str) and value.strip() and '\x00' not in value, 'Invalid ' + label)


def _reference(value):
    require(isinstance(value, str) and value.startswith('references/') and value.endswith('.md'),
            'Contracts must name Markdown under references/')
    return value


def _source_bytes(skill, relative):
    path = _path(skill, relative, required=True)
    observed = _snapshot(skill, path)
    require(observed is not None, 'Source disappeared: ' + relative)
    return observed[-1]


def _manifest(skill):
    try:
        data = json.loads(_source_bytes(skill, 'agents/roles.json').decode('utf-8'), object_pairs_hook=_pairs)
    except (json.JSONDecodeError, UnicodeError) as exc:
        raise GenerationError('Invalid role manifest JSON: ' + str(exc)) from exc
    _keys(data, {'schema_version', 'shared', 'contracts', 'host_settings', 'permission_profiles', 'roles'}, 'manifest')
    require(type(data['schema_version']) is int and data['schema_version'] == 1, 'Unsupported role manifest version')
    _reference(data['shared'])
    require(isinstance(data['contracts'], dict) and data['contracts'], 'Missing role contracts')
    for key, relative in data['contracts'].items():
        require(re.fullmatch(r'[a-z][a-z0-9_-]*', key), 'Invalid contract identifier')
        _reference(relative)
    settings = data['host_settings']
    _keys(settings, HOSTS, 'host settings')
    for host in ('claude', 'copilot'):
        _keys(settings[host], {'tools'}, host + ' settings')
        tools = settings[host]['tools']
        require(isinstance(tools, list) and tools and all(isinstance(t, str) and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_.:()/\-*]*', t) for t in tools)
                and len(tools) == len(set(tools)), 'Invalid ' + host + ' tool allowlist')
    _keys(settings['codex'], {'web_search', 'features'}, 'codex settings')
    require(settings['codex']['web_search'] == 'disabled', 'Unsupported codex web search setting')
    _keys(settings['codex']['features'], {'shell_tool'}, 'codex features')
    require(type(settings['codex']['features']['shell_tool']) is bool, 'shell_tool must be boolean')
    _keys(settings['pi'], set(), 'pi settings')
    permissions = data['permission_profiles']
    require(isinstance(permissions, dict) and permissions, 'Missing permission profiles')
    for key, profile in permissions.items():
        require(re.fullmatch(r'[a-z][a-z0-9_-]*', key), 'Invalid permission profile name')
        _keys(profile, {'sandbox_mode'}, 'permission profile')
        require(isinstance(profile['sandbox_mode'], str) and profile['sandbox_mode'] in {'read-only', 'workspace-write'}, 'Unsupported sandbox mode')
    require(isinstance(data['roles'], list) and data['roles'], 'Missing roles')
    names = set()
    for role in data['roles']:
        require(isinstance(role, dict), 'Role must be an object')
        require(set(role) in ({'name', 'description', 'contract', 'permissions'},
                             {'name', 'description', 'contract', 'permissions', 'discipline'}), 'Invalid role fields')
        require(isinstance(role['name'], str) and re.fullmatch(NAME, role['name']) and role['name'] not in names,
                'Invalid or duplicate role name')
        names.add(role['name'])
        _text(role['description'], 'role description')
        require(isinstance(role['contract'], str) and role['contract'] in data['contracts'], 'Unknown role contract')
        require(isinstance(role['permissions'], str) and role['permissions'] in permissions, 'Unknown role permission profile')
        if role['contract'] == 'reviewer':
            require(isinstance(role.get('discipline'), str) and role['discipline'] in DISCIPLINES, 'Reviewer needs a known discipline')
        else:
            require('discipline' not in role, 'Only reviewer roles declare a discipline')
    for relative in {data['shared'], *data['contracts'].values()}:
        require(_source_bytes(skill, relative).decode('utf-8').strip(), 'Empty canonical contract: ' + relative)
    return data


def _sources(skill, manifest):
    """Hash semantic manifest metadata plus every referenced-contract tree file."""
    references = skill / 'references'
    require(references.is_dir() and not references.is_symlink(), 'Missing or symlinked references directory')
    items = [('agents/roles.json', json.dumps(manifest, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode('utf-8'))]
    for folder, dirs, files in os.walk(references, followlinks=False):
        for name in dirs:
            require(not (Path(folder) / name).is_symlink(), 'Symlink reference directory is forbidden')
        for name in files:
            if name.endswith('.md'):
                relative = (Path(folder) / name).relative_to(skill).as_posix()
                raw = _source_bytes(skill, relative)
                raw.decode('utf-8')
                items.append((relative, raw))
    digest = hashlib.sha256(b'engram-factory-agent-bundle-v1\0')
    for relative, raw in sorted(items):
        encoded = relative.encode('utf-8')
        digest.update(len(encoded).to_bytes(8, 'big'))
        digest.update(encoded)
        digest.update(len(raw).to_bytes(8, 'big'))
        digest.update(raw)
    return digest.hexdigest()


def _body(manifest, role, digest):
    text = '<!-- ' + MARKER + digest + ' -->\n\n'
    text += 'Read and follow BOTH canonical contracts before acting:\n'
    text += '- Shared: `' + PREFIX + manifest['shared'] + '`.\n'
    text += '- Role: `' + PREFIX + manifest['contracts'][role['contract']] + '`.\n'
    if 'discipline' in role:
        text += '\nAssigned discipline: ' + role['discipline'] + '.\n'
    return text


def _wrapper(manifest, role, host, digest):
    body = _body(manifest, role, digest)
    quote = lambda value: json.dumps(value, ensure_ascii=True)
    if host == 'codex':
        text = '# ' + MARKER + digest + '\n'
        text += 'name: ' + role['name'] + '\ndescription: ' + quote(role['description']) + '\n'
        text += 'sandbox_mode: ' + manifest['permission_profiles'][role['permissions']]['sandbox_mode'] + '\n'
        text += 'web_search: ' + manifest['host_settings']['codex']['web_search'] + '\n'
        text += 'developer_instructions: |\n'
        text += ''.join('  ' + line + '\n' if line else '\n' for line in body.rstrip('\n').split('\n'))
        text += '\nfeatures:\n  shell_tool: ' + str(manifest['host_settings']['codex']['features']['shell_tool']).lower() + '\n'
        return text.encode('utf-8')
    text = '---\nname: ' + role['name'] + '\ndescription: ' + quote(role['description']) + '\n'
    if host in ('claude', 'copilot'):
        text += 'tools: ' + ', '.join(manifest['host_settings'][host]['tools']) + '\n'
    text += '---\n\n' + body
    return text.encode('utf-8')


def _managed(raw):
    try:
        return re.search(re.escape(MARKER) + r'[0-9a-f]{64}', raw.decode('utf-8')) is not None
    except UnicodeError:
        return False


def _outputs(skill, repo, manifest, digest):
    result = {}
    for role in manifest['roles']:
        for host in HOSTS:
            suffix = '.codex.yaml' if host == 'codex' else '.' + host + '.md'
            target = _path(skill, 'agents/' + role['name'] + suffix)
            result[target] = _wrapper(manifest, role, host, digest)
        target = _path(repo, '.pi/agents/' + role['name'] + '.md')
        result[target] = _wrapper(manifest, role, 'pi', digest)
    return result


@contextmanager
def _parent(root, path, create=False):
    """Anchor every component with no-follow directory descriptors (POSIX)."""
    descriptor = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for index, component in enumerate(path.parent.parts[1:]):
            try:
                child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            except FileNotFoundError:
                require(create and index >= len(root.parts) - 1, 'Output parent disappeared: ' + str(path.parent))
                try:
                    os.mkdir(component, dir_fd=descriptor)
                except FileExistsError:
                    pass
                child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        yield descriptor
    finally:
        os.close(descriptor)


def _read_at(parent, name):
    try:
        descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
    except FileNotFoundError:
        return None
    with os.fdopen(descriptor, 'rb') as stream:
        before = os.fstat(stream.fileno())
        require(stat.S_ISREG(before.st_mode), 'Agent output is not a regular file: ' + name)
        raw = stream.read()
        after = os.fstat(stream.fileno())
    identity = lambda info: (info.st_dev, info.st_ino, info.st_mtime_ns, info.st_size, info.st_mode)
    require(identity(before) == identity(after), 'Agent output changed while reading: ' + name)
    return (*identity(after), raw)


def _snapshot(root, path):
    _path(root, path.relative_to(root).as_posix())
    if not path.parent.exists():
        return None
    with _parent(root, path) as parent:
        return _read_at(parent, path.name)


def _unexpected(skill, repo, expected):
    result = {}
    for root, relative, pattern in ((skill, 'agents', NAME + r'\.(?:claude\.md|codex\.yaml|copilot\.md|pi\.md)'),
                                    (repo, '.pi/agents', NAME + r'\.md')):
        directory = root / relative
        _path(root, relative + '/.generation-path-check')
        if not directory.exists():
            continue
        for path in sorted(directory.iterdir()):
            if re.fullmatch(pattern, path.name) and path not in expected:
                observed = _snapshot(root, path)
                if observed is not None and _managed(observed[-1]):
                    result[path] = observed
    return result


def _verify_outputs(skill, repo, expected, observed, unexpected, require_expected=False):
    for path in expected:
        root = skill if path.is_relative_to(skill) else repo
        current = _snapshot(root, path)
        require(current == observed[path], 'Agent output changed after scan: ' + str(path))
        if require_expected:
            require(current is not None and current[-1] == expected[path], 'Generated agent bytes differ from expected output: ' + str(path))
    require(_unexpected(skill, repo, expected) == unexpected, 'Generated output inventory changed after scan')


def _write(root, path, data, observed):
    # Existing managed replacement requires a quiescent writer checkpoint: POSIX
    # has no portable compare-and-swap replacement/unlink. Identity checks detect
    # ordinary concurrent edits; they do not authenticate a hostile filesystem.
    with _parent(root, path, create=True) as parent:
        require(_read_at(parent, path.name) == observed, 'Agent output changed before write: ' + str(path))
        temporary = '.generated-agent-' + secrets.token_hex(16)
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        try:
            with os.fdopen(descriptor, 'wb') as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            require(_read_at(parent, path.name) == observed, 'Agent output changed before replacement: ' + str(path))
            if observed is None:
                # New files never replace a concurrently created foreign file.
                os.link(temporary, path.name, src_dir_fd=parent, dst_dir_fd=parent, follow_symlinks=False)
            else:
                require(_managed(observed[-1]), 'Refusing foreign output replacement')
                os.replace(temporary, path.name, src_dir_fd=parent, dst_dir_fd=parent)
            written = _read_at(parent, path.name)
            require(written is not None and written[-1] == data, 'Generated agent bytes changed before write confirmation: ' + str(path))
            return written
        finally:
            try:
                os.unlink(temporary, dir_fd=parent)
            except FileNotFoundError:
                pass


def _prune(root, path, observed):
    with _parent(root, path) as parent:
        require(_read_at(parent, path.name) == observed and _managed(observed[-1]),
                'Generated output ownership changed before pruning: ' + str(path))
        os.unlink(path.name, dir_fd=parent)


def generate(skill_root, repo_root=None, check=False, expected_digest=None, digest_only=False):
    require(not (check and digest_only), '--check and --digest are mutually exclusive')
    require(expected_digest is None or check or digest_only, '--expected-digest requires --check or --digest')
    require(expected_digest is None or (isinstance(expected_digest, str) and re.fullmatch(r'[0-9a-f]{64}', expected_digest)),
            'Invalid expected bundle digest')
    skill = _root(skill_root)
    manifest = _manifest(skill)
    digest = _sources(skill, manifest)
    if digest_only:
        require(_sources(skill, _manifest(skill)) == digest, 'Canonical sources changed during digest check; retry at a stable checkpoint')
        matches = expected_digest is None or expected_digest == digest
        return {'bundle_sha256': digest, 'roles': len(manifest['roles']), 'expected_digest_matches': matches,
                'status': 'current' if matches else 'drift'}
    require(repo_root is not None, 'Generation/check requires --repo-root')
    repo = _root(repo_root)
    expected = _outputs(skill, repo, manifest, digest)
    stale, foreign, observed = [], [], {}
    for path, raw in expected.items():
        root = skill if path.is_relative_to(skill) else repo
        snapshot = _snapshot(root, path)
        observed[path] = snapshot
        if snapshot is None or snapshot[-1] != raw:
            stale.append(path)
            if snapshot is not None and not _managed(snapshot[-1]):
                foreign.append(path)
    unexpected = _unexpected(skill, repo, expected)
    mismatch = expected_digest is not None and expected_digest != digest
    require(_sources(skill, _manifest(skill)) == digest, 'Canonical sources changed during generation/check; retry at a stable checkpoint')
    summary = {'bundle_sha256': digest, 'roles': len(manifest['roles']), 'wrappers': len(manifest['roles']) * 4,
               'pi_mirrors': len(manifest['roles']), 'stale': [str(p) for p in stale],
               'unexpected_generated': [str(p) for p in unexpected], 'expected_digest_matches': not mismatch}
    _verify_outputs(skill, repo, expected, observed, unexpected)
    if check:
        summary['status'] = 'drift' if stale or unexpected or mismatch else 'current'
        return summary
    require(not foreign, 'Refusing to overwrite foreign agent files: ' + ', '.join(str(p) for p in foreign))
    # No filesystem mutation occurs before every input/output/conflict has been checked.
    for path in stale:
        root = skill if path.is_relative_to(skill) else repo
        observed[path] = _write(root, path, expected[path], observed[path])
    for path, prior in unexpected.items():
        root = skill if path.is_relative_to(skill) else repo
        _prune(root, path, prior)
    require(_sources(skill, _manifest(skill)) == digest, 'Canonical sources changed during generation; preserve outputs and retry at a stable checkpoint')
    _verify_outputs(skill, repo, expected, observed, {}, require_expected=True)
    summary.update(status='generated', written=len(stale), pruned=len(unexpected), stale=[], unexpected_generated=[])
    return summary


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skill-root', default=str(Path(__file__).absolute().parents[1]))
    parser.add_argument('--repo-root', default=str(Path(__file__).absolute().parents[4]))
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--digest', action='store_true', help='Read canonical bundle identity without checking repository outputs')
    parser.add_argument('--expected-digest')
    args = parser.parse_args(argv)
    try:
        result = generate(args.skill_root, args.repo_root, args.check, args.expected_digest, args.digest)
    except (GenerationError, OSError, UnicodeError) as exc:
        print(json.dumps({'error': str(exc)}), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2))
    return 1 if result['status'] == 'drift' else 0


if __name__ == '__main__':
    sys.exit(main())
