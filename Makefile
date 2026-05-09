build:
	@echo Building alpathfinder...
	wasm-pack build --target nodejs
	@echo Adding typed-adventureland dependency to package.json...
	@node -e " \
		const fs = require('fs'); \
		const pkg = JSON.parse(fs.readFileSync('pkg/package.json', 'utf8')); \
		pkg.dependencies = { 'typed-adventureland': '^0.0.55' }; \
		fs.writeFileSync('pkg/package.json', JSON.stringify(pkg, null, 2)); \
	"
	@echo Finished!
