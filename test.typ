#import "setup.typ": *


= Aufgabe


== Beschreibung

*Neukonzipierung einer parallelen Punktwolkenverarbeitung mit Mehrkern-CPUs.*

Die Aufgaben des Praktikanten beinhalten das Entwickeln von effizienten Algorithmen und Datenstrukturen für die Verarbeitung großer Laserscans mit mindestens folgender Funktionalität: Glätten (Tiefpassfilter), eliminieren von Ausreissern, Datenkompression (lossy mit Genauigkeitsvorgaben), adaptive Triangulierung. Optional (je nach Fortschritt) soll auch eine Segmentierung der Punktewolken durchgeführt werden.

Herr Wetzel hat bereits in seiner Bachelorarbeit Erfahrung mit Punktewolkenverarbeitung sammeln können. Dies beschränkte sich allerdings auf eine spezielle Programmierumgebung "WebGPU" auf hochparallelen GPU's mit tausenden Rechenkernen, einhergehend mit bekannten Einschränkungen bei der Programmierflexibilität aufgrund der speziellen (SIMD) GPU-Rechenarchitektur, sowie Speicherbeschränkung auf wenige 100 000 Punkte.

Die Anforderungen im Industriepraktikum sind wesentlich andere als in der Bachelorarbeit. Es geht hierbei um die Verarbeitung von Milliarden Punkten. Konkret sind Modelle mit über 100 Milliarden Punkten (und somit Millionen mal so viele Daten) zu verarbeiten. Die Datenmengen liegen typischerweise im Terabyte-Bereich und passen nicht in den Hauptspeicher eines PCs und schon gar nicht in den Speicher selbst großer Grafikkarten.

Für die Realisierung steht Herrn Wetzel eine Out-of-Core-Speicherverwaltung der Fa. 3DInteractive zur Verfügung. Diese Umgebung übernimmt das Einlesen der Daten aus unterschiedlichen Scannerformaten, sowie eine räumliche Vorsortierung und das Zwischenspeichern in einer räumlichen Datenbank. Diese Software kann über eine API angesteuert werden.

Eine wichtige Anforderung an die neu zu entwickelnden Software ist die Unterstützung der Parallelverarbeitung (thread safety). Die Software soll im Gesamtsystem integriert und mit realen Datensätzen von Industriekunden ausgewertet werden. Die Programmiersprache ist C++.


== Existierende Software

Die Algorithmen sind als Erweiterung von bereits existierender Software eingeplant. Dabei handelt es sich um den Importer für Punktwolken. Dieser liest Punktwolken in unterschiedlichen Datenformaten ein, kombiniert diese, berechnet weitere Eigenschaften und speichert die Punktwolke in ein gemeinsames Format für die Darstellung. Das Format für die Darstellung ist unabhängig von den Eingabeformaten.


== Erweiterung

Bei der Bearbeitung der Aufgabe sind Lösungen für weitere Probleme entstanden. Dazu gehören Alternativen für die Berechnung der Normalen und Punktgrößen und die Berechnung der Detailstufen für das Anzeigen der Punkte.


== Betrieb

3DInteractive würde 2004 gegründet und veröffentlicht die erste Software in 2005. Momentan hat die Firma zwischen 5 und 25 Mitarbeiter.

Es wird Software für die Luft- und Raumfahrt, Automobilindustrie, Schiffsbau, digitale Fabrikplanung und weitere Gebiete entwickelt. Dazu gehören die Programme _VGR_ und _LSB_ für die Visualisierung in Echtzeit von mehreren großen CAD-Modellen, Visualisierung von Punktwolken mit Milliarden Punkten und weiterführende Analyse der Daten.

#figure(
	caption: [Unterschiedliche Visualisierungen von großen Modellen. @3di],
	grid(
		columns: (1fr, 1fr),
		image(width: 90%, "bilder/vgr_1.png"), image(width: 90%, "bilder/vgr_2.png"),
	),
)

#todo[Erlaubnis Bilder und Quelle besser]

Das Programm _LSB_ für die Verarbeitung von Punktwolken importiert zuerst die Punktdaten aus unterschiedlichen Formaten und speichert diese in ein internes Format ab. Dieses wird für die weitere Visualisierung verwendet.
