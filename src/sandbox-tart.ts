import { execFile } from "child_process";

export interface TartVm {
  name: string;
  state: string;
}

export interface RunCommandResult {
  stdout: string;
  stderr: string;
}

export type RunCommand = (
  cmd: string,
  args: string[],
  opts?: { cwd?: string; env?: NodeJS.ProcessEnv },
) => Promise<RunCommandResult>;

const defaultRunCommand: RunCommand = (cmd, args, opts) =>
  new Promise((resolve, reject) => {
    execFile(
      cmd,
      args,
      { ...opts, maxBuffer: 16 * 1024 * 1024 },
      (err, stdout, stderr) => {
        if (err) {
          (err as NodeJS.ErrnoException & { stderr?: string }).stderr = String(stderr);
          reject(err);
          return;
        }
        resolve({ stdout: String(stdout), stderr: String(stderr) });
      },
    );
  });

export interface TartCliOptions {
  runCommand?: RunCommand;
  binPath?: string;
}

export class TartCli {
  private readonly runCmd: RunCommand;
  private readonly bin: string;

  constructor(opts: TartCliOptions = {}) {
    this.runCmd = opts.runCommand ?? defaultRunCommand;
    this.bin = opts.binPath ?? "tart";
  }

  async version(): Promise<string> {
    const { stdout } = await this.runCmd(this.bin, ["--version"]);
    return stdout.trim();
  }

  async clone(base: string, vmName: string): Promise<void> {
    await this.runCmd(this.bin, ["clone", base, vmName]);
  }

  async delete(vmName: string): Promise<void> {
    await this.runCmd(this.bin, ["delete", vmName]);
  }

  async listVms(): Promise<TartVm[]> {
    const { stdout } = await this.runCmd(this.bin, ["list", "--format", "json"]);
    try {
      const arr = JSON.parse(stdout) as Array<{ Name: string; State: string }>;
      return arr.map((r) => ({ name: r.Name, state: r.State }));
    } catch {
      return [];
    }
  }
}
