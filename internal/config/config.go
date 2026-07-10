package config

type Device struct {
	ID             string `yaml:"id"`
	Name           string `yaml:"name"`
	PrivateKeyPath string `yaml:"private_key_path"`
}
type Relay struct {
	URL      string `yaml:"url"`
	SignalWS string `yaml:"signal_ws"`
}
type Agent struct {
	TargetBaseURL string   `yaml:"target_base_url"`
	AllowHosts    []string `yaml:"allow_hosts"`
}
type Proxy struct {
	Listen string `yaml:"listen"`
}
type Remote struct {
	DeviceID string `yaml:"device_id"`
}
type Config struct {
	Device Device `yaml:"device"`
	Relay  Relay  `yaml:"relay"`
	Agent  Agent  `yaml:"agent"`
	Proxy  Proxy  `yaml:"proxy"`
	Remote Remote `yaml:"remote"`
}
