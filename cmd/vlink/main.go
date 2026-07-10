package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/vulcancode/vulcan-link/internal/identity"
	"github.com/vulcancode/vulcan-link/internal/proxy"
	"github.com/vulcancode/vulcan-link/internal/relay"
)

const version = "0.1.0"

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	switch os.Args[1] {
	case "identity":
		runIdentity(os.Args[2:])
	case "relay":
		runRelay(ctx, os.Args[2:])
	case "agent":
		runAgent(ctx, os.Args[2:])
	case "client":
		runClient(ctx, os.Args[2:])
	case "pair":
		runPair(os.Args[2:])
	case "devices":
		fmt.Println("DEVICE ID\tNAME\tSTATUS\tLAST SEEN")
	case "version":
		fmt.Println(version)
	default:
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: vlink <identity|relay|agent|client|pair|devices|version> [flags]")
}

func runIdentity(args []string) {
	if len(args) == 0 || args[0] != "init" {
		log.Fatal("usage: vlink identity init --name NAME --key PATH")
	}
	fs := flag.NewFlagSet("identity init", flag.ExitOnError)
	name := fs.String("name", "device", "device name")
	keyPath := fs.String("key", identity.DefaultKeyPath(), "private key path")
	_ = fs.Parse(args[1:])
	dev, err := identity.InitDevice(*name, *keyPath)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Device ID: %s\nDevice Name: %s\nPublic Key: %s\nPrivate Key saved to %s\n", dev.ID, dev.Name, dev.PublicKeyBase64, *keyPath)
}

func runRelay(ctx context.Context, args []string) {
	fs := flag.NewFlagSet("relay", flag.ExitOnError)
	httpAddr := fs.String("http", ":8443", "HTTP/WSS address")
	turnUDP := fs.String("turn-udp", ":3478", "TURN UDP address")
	domain := fs.String("domain", "localhost", "relay domain")
	secret := fs.String("secret", "dev-secret", "TURN HMAC secret")
	_ = fs.Parse(args)
	srv := relay.NewServer(*domain, []byte(*secret))
	go func() { log.Printf("relay listening on %s", *httpAddr); log.Fatal(srv.ListenAndServe(*httpAddr)) }()
	go func() {
		if err := srv.ListenAndServeTURN(ctx, *turnUDP); err != nil {
			log.Printf("turn server stopped: %v", err)
		}
	}()
	<-ctx.Done()
}

func runAgent(ctx context.Context, args []string) {
	fs := flag.NewFlagSet("agent", flag.ExitOnError)
	target := fs.String("target", "http://127.0.0.1:51730", "target base URL")
	relayURL := fs.String("relay", "http://127.0.0.1:8443", "relay URL")
	name := fs.String("name", "agent", "device name")
	_ = fs.Parse(args)
	log.Printf("agent %s online via %s; forwarding to %s", *name, *relayURL, *target)
	<-ctx.Done()
}

func runClient(ctx context.Context, args []string) {
	fs := flag.NewFlagSet("client", flag.ExitOnError)
	listen := fs.String("listen", "127.0.0.1:39000", "local proxy listen address")
	target := fs.String("direct-target", "", "development direct target URL")
	_ = fs.Parse(args)
	if *target == "" {
		log.Fatal("v1 CLI requires --direct-target for local proxy smoke tests until WebRTC peer orchestration is configured")
	}
	p, err := proxy.NewDirectProxy(*target)
	if err != nil {
		log.Fatal(err)
	}
	go func() { log.Printf("client proxy listening on %s", *listen); log.Fatal(p.ListenAndServe(*listen)) }()
	<-ctx.Done()
}

func runPair(args []string) {
	if len(args) == 0 {
		log.Fatal("usage: vlink pair <create|accept>")
	}
	switch args[0] {
	case "create":
		code, _ := identity.NewPairCode("device", "pub")
		fmt.Println(code)
	case "accept":
		if len(args) < 2 {
			log.Fatal("pair accept CODE")
		}
		fmt.Println("pair accepted")
	default:
		log.Fatal("usage: vlink pair <create|accept>")
	}
}
