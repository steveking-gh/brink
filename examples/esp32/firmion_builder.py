# This weird import is specific to Scons builds
Import("env")
import os
import subprocess

def run_firmion_builder(source, target, env):
    # 'source' contains the absolute path to the generated firmware.elf
    elf_file = source[0].get_abspath()

    # Define where Firmion should spit out the final image
    # We will name it firmware.firmion.bin to sit next to the default output
    build_dir = env.subst("$BUILD_DIR")
    output_bin = os.path.join(build_dir, "firmware.firmion.bin")

    print("\n" + "="*50)
    print("🚀 INTERCEPTED BY FIRMION")
    print(f"Target ELF: {elf_file}")
    print(f"Output BIN: {output_bin}")

    # Execute Firmion (assuming firmion is in your system PATH)
    # You may need to pass the ELF path to your .firmion script via env vars
    # or command line arguments if Firmion supports them.
    firmion_script = "ESP32_S3_test.firmion"

    # Ask PlatformIO for the build directory and the program name (usually 'firmware')
    build_dir = env.subst("$BUILD_DIR")
    print("Build Directory:", build_dir)
    prog_name = env.subst("${PROGNAME}")
    print("Program Name:", prog_name)

    # Construct the absolute path to the final .elf file Firmion needs to process
    elf_file = os.path.join(build_dir, f"{prog_name}.elf")
    print("Final ELF Path:", elf_file)

    try:
        # Example execution: firmion layout.firmion -o output.bin
        cmd = ["firmion", firmion_script,
               "-D", f'FIRMWARE_PATH="{elf_file}"',
               "-o", output_bin]
        print(f"Running: {' '.join(cmd)}")

        result = subprocess.run(cmd, check=True, text=True, capture_output=True)
        print(result.stdout)
        print("✅ Firmion Image Generation Successful")

        # Firmware file renaming
        import shutil

        build_dir = env.subst("$BUILD_DIR")
        prog_name = env.subst("${PROGNAME}")

        default_bin = os.path.join(build_dir, f"{prog_name}.bin")
        esptool_bin = os.path.join(build_dir, f"{prog_name}.esptool.bin")
        firmion_bin = os.path.join(build_dir, "firmware.firmion.bin")

        # 1. Preserve the original esptool binary so you have both
        if os.path.exists(default_bin) and not os.path.exists(esptool_bin):
            shutil.move(default_bin, esptool_bin)
            print(f"Backed up esptool image to: {prog_name}.esptool.bin")
        elif os.path.exists(default_bin):
            # If esptool_bin already exists from a previous build, just overwrite it
            shutil.copy(default_bin, esptool_bin)

        # 2. Trick PlatformIO into uploading the Firmion binary
        if os.path.exists(firmion_bin):
            shutil.copy(firmion_bin, default_bin)
            print(f"Set Firmion image as the active {default_bin} upload target!")

    except subprocess.CalledProcessError as e:
        print("❌ Firmion Build Failed")
        print(e.stderr)
        # Force the PlatformIO build to fail if Firmion fails
        env.Exit(1)

    print("="*50 + "\n")

# Attach our custom function to run immediately after the ELF is built
env.AddPostAction("$BUILD_DIR/${PROGNAME}.bin", run_firmion_builder)