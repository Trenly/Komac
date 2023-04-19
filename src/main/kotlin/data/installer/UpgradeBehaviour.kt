package data.installer

import commands.interfaces.MenuPrompt
import data.AllManifestData
import data.PreviousManifestData
import schemas.manifest.InstallerManifest

object UpgradeBehaviour : MenuPrompt<InstallerManifest.UpgradeBehavior?> {
    override val name: String = "Upgrade behaviour"

    override val default: InstallerManifest.UpgradeBehavior = previousValue ?: InstallerManifest.UpgradeBehavior.Install

    @OptIn(ExperimentalStdlibApi::class)
    override val items = InstallerManifest.UpgradeBehavior.entries

    private val previousValue: InstallerManifest.UpgradeBehavior?
        get() = PreviousManifestData.installerManifest?.let {
            it.upgradeBehavior ?: it.installers.getOrNull(AllManifestData.installers.size)?.upgradeBehavior
        }
}
