// electron-builder runs this after it copies the app, before the .dmg is made. Apple silicon runs
// only signed code, and the app is not signed with a certificate yet (build order step 11), so it
// gets an ad hoc signature. That is enough on the Mac that built it.
const { execFileSync } = require('node:child_process')
const { join } = require('node:path')

exports.default = async function afterPack(context) {
  if (context.electronPlatformName !== 'darwin') {
    return
  }
  const app = join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`)
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', app], { stdio: 'inherit' })
}
