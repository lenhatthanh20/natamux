// electron-builder runs this after it copies the app, before the .dmg, AppImage or .deb is made.
const { execFileSync } = require('node:child_process')
const { join } = require('node:path')

exports.default = async function afterPack(context) {
  if (context.electronPlatformName === 'linux') {
    // The packages keep the folder modes of the build machine. A machine with an odd umask or
    // ACLs would make /opt/Natamux unreadable for other users, so the modes are set here.
    execFileSync('chmod', ['-R', 'u+rwX,go+rX,go-w', context.appOutDir])
    return
  }
  if (context.electronPlatformName !== 'darwin') {
    return
  }
  // Apple silicon runs only signed code, and the app is not signed with a certificate yet (build
  // order step 11), so it gets an ad hoc signature. That is enough on the Mac that built it.
  const app = join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`)
  execFileSync('codesign', ['--force', '--deep', '--sign', '-', app], { stdio: 'inherit' })
}
