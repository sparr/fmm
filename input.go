package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
)

type ModIdentAndPresence struct {
	Ident     ModIdent
	IsPresent bool
}

func parseCliInput(input []string, parseDependencies bool) []ModIdentAndPresence {
	var mods []ModIdent

	for _, input := range input {
		if strings.HasSuffix(input, ".zip") {
			// TODO: Read from save
		} else if strings.HasSuffix(input, ".log") {
			mods = append(mods, parseLogFile(input)...)
		} else if strings.HasSuffix(input, ".json") {
			// TODO: mod-list.json
		} else if strings.HasPrefix(input, "!") {
			// TODO: Mod set
		} else {
			mods = append(mods, newModIdent(input))
		}
	}

	if parseDependencies {
		mods = expandDependencies(mods)
	}

	var output []ModIdentAndPresence

	dir := newDir(modsDir)

	for _, mod := range mods {
		present := dir.Find(Dependency{mod, DependencyRequired, VersionAny}) != nil
		output = append(output, ModIdentAndPresence{mod, present})
	}

	return output
}

func expandDependencies(mods []ModIdent) []ModIdent {
	visited := make(map[string]bool)
	toVisit := []Dependency{}
	for _, mod := range mods {
		toVisit = append(toVisit, Dependency{mod, DependencyRequired, VersionEq})
	}
	output := []ModIdent{}

	dir := newDir(modsDir)

	for i := 0; i < len(toVisit); i += 1 {
		mod := toVisit[i]
		if _, exists := visited[mod.Ident.Name]; exists {
			continue
		}
		visited[mod.Ident.Name] = true
		var ident ModIdent
		var deps []Dependency
		var err error
		if file := dir.Find(mod); file != nil {
			ident = file.Ident
			deps, err = file.Dependencies()
		} else {
			var release *PortalModRelease
			release, err = portalGetRelease(mod)
			if err == nil {
				ident = ModIdent{mod.Ident.Name, &release.Version}
				deps = release.InfoJson.Dependencies
			}
		}
		if err != nil {
			errorln(err)
			continue
		}
		output = append(output, ident)
		for _, dep := range deps {
			if dep.Ident.Name == "base" {
				continue
			}
			if dep.Kind == DependencyRequired || dep.Kind == DependencyNoLoadOrder {
				toVisit = append(toVisit, dep)
			}
		}
	}

	return output
}

func parseLogFile(filepath string) []ModIdent {
	var output []ModIdent
	file, err := os.Open(filepath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error reading %s: %s", filepath, err)
		return output
	}
	defer file.Close()

	fileScanner := bufio.NewScanner(file)
	fileScanner.Split(bufio.ScanLines)

	inChecksums := false
	for fileScanner.Scan() {
		line := fileScanner.Text()
		if !strings.Contains(line, "Checksum of") {
			if inChecksums {
				break
			} else {
				continue
			}
		}
		inChecksums = true
		parts := strings.Split(strings.TrimSpace(line), " ")
		modName, _ := strings.CutSuffix(strings.Join(parts[3:len(parts)-1], " "), ":")
		if modName == "base" {
			continue
		}
		output = append(output, ModIdent{modName, nil})
	}

	return output
}
