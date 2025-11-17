## Roadmap

[ ] Backup files in stash before running commands, revert to previous state if command fails
[ ] Add ability to define command timeout
[ ] Groups in config, each group has list of patterns to watch and own settings for group like execution order
[ ] Add ability to define execution order, parallel or sequential in each group
[ ] Add ability to define timeout of command in each group
[ ] Add ability to define command execution behavior, run on each file or pass a list of files to command in each group
[ ] Add ability to define continue execution of commands if any command fails
[ ] Display total affected files count
[ ] Display total time of execution
[ ] Display total time of execution per command
[ ] Display error stderr and sdout in block with border
[ ] Add ability to define relative or absolute file paths will be passed to command for each group
[ ] Add config variations, read .fast-staged.toml, or fast-staged.toml or read fast-staged.json or .fast-staged.json or read "fast-staged" section in package.json
[ ] Add checks and readable errors (create errors enum) for `no config`, `config is not valid`, `no git repository`, `no staged files found` , `no files found matched for group_name patterns`, `failed to execute command, no command found`,
