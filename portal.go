package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"os"
	"path"

	"github.com/cavaliergopher/grab/v3"
)

const initUploadUrl string = "https://mods.factorio.com/api/v2/mods/releases/init_upload"

func portalDownloadMod(mod Dependency) error {
	url := fmt.Sprintf("https://mods.factorio.com/api/mods/%s/full", mod.Ident.Name)
	res, err := http.Get(url)
	if err != nil {
		return err
	}

	body, err := io.ReadAll(res.Body)
	if err != nil {
		return err
	}
	res.Body.Close()

	var unmarshaled PortalFullMod
	err = json.Unmarshal(body, &unmarshaled)
	if err != nil {
		return err
	}

	// Check releases from newest to oldest and find the first matching one
	var release *PortalModRelease
	for i := len(unmarshaled.Releases) - 1; i >= 0; i -= 1 {
		toCheck := &unmarshaled.Releases[i]
		if mod.Test(&toCheck.Version) {
			release = toCheck
			break
		}
	}

	if release == nil {
		return errors.New(fmt.Sprintf("%s was not found on the mod portal", mod.Ident.toString()))
	}

	downloadUrl := fmt.Sprintf("https://mods.factorio.com/%s?username=%s&token=%s",
		release.DownloadUrl, downloadUsername, downloadToken)
	outPath := path.Join(modsDir, release.FileName)

	fmt.Printf("Downloading %s\n", release.FileName)
	if _, err := grab.Get(outPath, downloadUrl); err != nil {
		return err
	}

	return nil
}

func portalUploadMod(filepath string) error {
	// Init upload
	initUploadBody := &bytes.Buffer{}
	w := multipart.NewWriter(initUploadBody)
	ident := newModIdent(path.Base(filepath))
	w.WriteField("mod", ident.Name)
	w.Close()
	req, err := http.NewRequest("POST", initUploadUrl, initUploadBody)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", apiKey))
	req.Header.Set("Content-Type", w.FormDataContentType())
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	var decoded ModInitUploadRes
	err = json.NewDecoder(res.Body).Decode(&decoded)
	if err != nil {
		return err
	}
	if res.StatusCode != http.StatusOK {
		return errors.New(*decoded.Message)
	}
	defer res.Body.Close()

	// Open file
	file, err := os.Open(filepath)
	if err != nil {
		return err
	}
	defer file.Close()

	fmt.Printf("Uploading %s\n", filepath)

	// Upload file
	uploadBody := &bytes.Buffer{}
	w = multipart.NewWriter(uploadBody)
	part, err := w.CreateFormFile("file", path.Base(file.Name()))
	io.Copy(part, file)
	w.Close()

	r, err := http.NewRequest("POST", *decoded.UploadUrl, uploadBody)
	if err != nil {
		return err
	}
	r.Header.Add("Content-Type", w.FormDataContentType())
	http.DefaultClient.Do(r)

	return nil
}

func portalGetRelease(mod Dependency) (*PortalModRelease, error) {
	fmt.Println("Fetching dependencies for", mod.Ident.toString())
	url := fmt.Sprintf("https://mods.factorio.com/api/mods/%s/full", mod.Ident.Name)
	res, err := http.Get(url)
	if err != nil {
		return nil, err
	}
	if res.StatusCode != http.StatusOK {
		return nil, errors.New(fmt.Sprintf("%s was not found on the mod portal", mod.Ident.toString()))
	}

	body, err := io.ReadAll(res.Body)
	if err != nil {
		return nil, err
	}
	res.Body.Close()

	var unmarshaled PortalFullMod
	err = json.Unmarshal(body, &unmarshaled)
	if err != nil {
		return nil, err
	}

	releases := unmarshaled.Releases
	for i := len(releases) - 1; i >= 0; i-- {
		release := &releases[i]
		if mod.Test(&release.Version) {
			return release, nil
		}
	}

	return &unmarshaled.Releases[len(unmarshaled.Releases)-1], nil
}

type ModInitUploadRes struct {
	UploadUrl *string `json:"upload_url"`
	Message   *string // When an error occurs
}

type PortalFullMod struct {
	Name     string
	Releases []PortalModRelease
	Title    string
}

type PortalModRelease struct {
	DownloadUrl string   `json:"download_url"`
	FileName    string   `json:"file_name"`
	InfoJson    InfoJson `json:"info_json"`
	Version     Version
}
