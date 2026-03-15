package main

import "C"
import (
	"context"
	"fmt"
	"os"
	"path/filepath"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/include"
	"github.com/sagernet/sing-box/option"
	sjson "github.com/sagernet/sing/common/json"
)

var instance *box.Box

func logError(msg string) {
	exePath, _ := os.Executable()
	logPath := filepath.Join(filepath.Dir(exePath), "singbox_error.log")
	f, err := os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		return
	}
	defer f.Close()
	fmt.Fprintln(f, msg)
}

//export StartSingBox
func StartSingBox(configJson *C.char) C.int {
	configStr := C.GoString(configJson)

	logError("=== StartSingBox called ===")

	// Use include.Context to register all protocol registries
	ctx := include.Context(context.Background())

	opt, err := sjson.UnmarshalExtendedContext[option.Options](ctx, []byte(configStr))
	if err != nil {
		logError("Error parsing config: " + err.Error())
		return -1
	}

	b, err := box.New(box.Options{
		Context: ctx,
		Options: opt,
	})

	if err != nil {
		logError("Error creating sing-box: " + err.Error())
		return -2
	}

	err = b.Start()
	if err != nil {
		logError("Error starting sing-box: " + err.Error())
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
			logError("Error stopping sing-box: " + err.Error())
			return -1
		}
		instance = nil
		logError("sing-box stopped successfully")
	}
	return 0
}

func main() {}
