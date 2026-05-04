import { basename, join, parse as parsePath } from "path";
import { mkdir, copyFile, unlink, access, constants } from "fs/promises";
import { randomUUID } from "crypto";

export interface QuarantineMoveResult {
  quarantineFilePath: string;
  originalBaseName: string;
}

class FileMover {
  private readonly destination: string;

  constructor(destination: string) {
    this.destination = destination;
  }

  /**
   * Copy file into quarantine under a unique name, remove from source.
   */
  async move(source: string): Promise<QuarantineMoveResult> {
    const originalBaseName = basename(source);
    const quarantineName = `${randomUUID()}_${originalBaseName}`;
    const quarantineFilePath = join(this.destination, quarantineName);

    try {
      await this.ensureDirectory();
      await copyFile(source, quarantineFilePath);
      await unlink(source);
      console.log(`Moved to ${quarantineFilePath}`);
    } catch (e) {
      throw new Error(`Failed to move ${source} to ${this.destination}: ${e}`);
    }

    return { quarantineFilePath, originalBaseName };
  }

  /**
   * Copy file back to watch folder under original basename, then remove from quarantine.
   * If target path already exists, uses name_restored_<timestamp>.ext instead of overwriting.
   */
  async restoreToWatch(
    watchPath: string,
    quarantineFilePath: string,
    originalBaseName: string,
  ): Promise<{ restoredPath: string }> {
    const restoredPath = await this.resolveRestoreDestination(
      watchPath,
      originalBaseName,
    );

    try {
      await copyFile(quarantineFilePath, restoredPath);
      await unlink(quarantineFilePath);
      console.log(`Restored to ${restoredPath}`);
    } catch {
      throw new Error(
        `Failed to restore ${quarantineFilePath} to ${restoredPath}`,
      );
    }

    return { restoredPath };
  }

  async resolveRestoreDestination(
    watchPath: string,
    originalBaseName: string,
  ): Promise<string> {
    const primary = join(watchPath, originalBaseName);
    try {
      await access(primary, constants.F_OK);
    } catch {
      return primary;
    }

    const { name, ext } = parsePath(originalBaseName);
    const fallback = join(watchPath, `${name}_restored_${Date.now()}${ext}`);
    try {
      await access(fallback, constants.F_OK);
    } catch {
      return fallback;
    }

    return join(
      watchPath,
      `${name}_restored_${Date.now()}_${randomUUID().slice(0, 8)}${ext}`,
    );
  }

  async deleteFile(quarantineFilePath: string): Promise<void> {
    try {
      await unlink(quarantineFilePath);
      console.log(`Deleted quarantined file: ${quarantineFilePath}`);
    } catch {
      throw new Error(`Failed to delete ${quarantineFilePath}`);
    }
  }

  async ensureDirectory() {
    await mkdir(this.destination, {
      recursive: true,
    });
  }
}

export default FileMover;
