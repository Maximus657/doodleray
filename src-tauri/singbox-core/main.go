package main

import "C"
import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/include"
	"github.com/sagernet/sing-box/option"
	sjson "github.com/sagernet/sing/common/json"
)

var instance *box.Box

func logError(msg string) {
	exePath, _ := os.Executable()
	logPath := filepath.Join(filepath.Dir(exePath), "singbox_error.log")
	f, err := os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0600)
	if err != nil {
		return
	}
	defer f.Close()
	_ = f.Chmod(0600)
	fmt.Fprintf(f, "%s %s\n", time.Now().UTC().Format(time.RFC3339), msg)
}

//export StartSingBox
func StartSingBox(configJson *C.char) C.int {
	configStr := C.GoString(configJson)

	logError("=== StartSingBox called ===")

	// Close any previous instance first
	if instance != nil {
		instance.Close()
		instance = nil
		time.Sleep(300 * time.Millisecond)
	}

	// Use include.Context to register all protocol registries
	ctx := include.Context(context.Background())

	opt, err := sjson.UnmarshalExtendedContext[option.Options](ctx, []byte(configStr))
	if err != nil {
		// Never log the parser error or raw configuration: either can expose
		// server addresses, UUIDs, credentials, and routing metadata.
		logError("Error parsing sing-box configuration")
		return -1
	}

	b, err := box.New(box.Options{
		Context: ctx,
		Options: opt,
	})

	if err != nil {
		logError("Error creating sing-box instance")
		return -2
	}

	err = b.Start()
	if err != nil {
		logError("Error starting sing-box instance")
		b.Close() // Clean up to release bound ports
		return -3
	}

	logError("sing-box started successfully")
	instance = b
	return 0
}

//export StopSingBox
func StopSingBox() C.int {
	if instance != nil {
		err := instance.Close()
		if err != nil {
			logError("Error stopping sing-box instance")
			return -1
		}
		instance = nil
		logError("sing-box stopped successfully")
	}
	return 0
}

func main() {}
