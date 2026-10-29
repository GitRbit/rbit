const fs = require('fs');
const value = Number(process.env['INPUT_VALUE']);
console.log('main ran with ' + value);
fs.appendFileSync(process.env.GITHUB_OUTPUT, `doubled=${value * 2}\n`);
fs.appendFileSync(process.env.GITHUB_STATE, `seen=${value}\n`);
